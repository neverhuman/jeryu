//! Create-once bootstrap account and credential-receipt handling.

use super::http::chrono_like_now;
use super::*;

#[derive(Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapCredential {
    login: String,
    role: String,
    password: String,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapCredentialFile {
    generated_at: String,
    credentials: Vec<BootstrapCredential>,
}

pub(super) fn bootstrap_public_accounts(
    state: &WebState,
    data_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let admin_password = match std::env::var(BOOTSTRAP_ADMIN_PASSWORD_ENV) {
        Ok(password) => Some(password),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(Box::new(error)),
    };
    bootstrap_public_accounts_with_admin_password(state, data_dir, admin_password.as_deref())
}

pub(super) fn bootstrap_public_accounts_with_admin_password(
    state: &WebState,
    data_dir: &Path,
    admin_password: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(password) = admin_password {
        create_or_reset_bootstrap_admin(state, password)?;
    }
    if admin_password.is_none() && state.core.get_account(BOOTSTRAP_ADMIN_LOGIN).is_err() {
        // Persist the recoverable password before committing the account. A
        // restart after receipt creation resumes with those exact credentials.
        let password = prepare_bootstrap_password(state, data_dir)?;
        state
            .core
            .create_temporary_account(BOOTSTRAP_ADMIN_LOGIN, &password, UserRole::Admin)?;
    }

    for repo in state.core.list_repositories(Some("jeryu")) {
        let split = state
            .split_catalog
            .classify(&repo.owner, &repo.name)
            .map(|(family, _)| family == "jeryu-split")
            .unwrap_or(false)
            || repo.family.as_deref() == Some("jeryu-split");
        if split {
            state.core.grant_repo_access(
                "bootstrap",
                BOOTSTRAP_ADMIN_LOGIN,
                &repo.owner,
                &repo.name,
                jeryu_core::RepoAccessLevel::Admin,
            )?;
        }
    }

    Ok(())
}

pub(super) fn prepare_bootstrap_password(
    state: &WebState,
    data_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join("bootstrap-credentials.json");
    match std::fs::symlink_metadata(&path) {
        Ok(_) => return read_bootstrap_password(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let password = state.core.generate_one_time_password()?;
    let receipt = BootstrapCredentialFile {
        generated_at: chrono_like_now(),
        credentials: vec![BootstrapCredential {
            login: BOOTSTRAP_ADMIN_LOGIN.to_string(),
            role: "admin".to_string(),
            password: password.clone(),
        }],
    };
    let json = serde_json::to_vec_pretty(&receipt)?;
    let mut file = secure_create(&path)?;
    file.write_all(&json)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::File::open(data_dir)?.sync_all()?;
    eprintln!(
        "jeryu: first-login receipt for {BOOTSTRAP_ADMIN_LOGIN} written to {} (owner-only; the password is not logged)",
        path.display()
    );
    Ok(password)
}

fn read_bootstrap_password(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use rustix::fs::{Mode, OFlags, open};
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;

    let fd = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )?;
    let mut file = std::fs::File::from(fd);
    let before = file.metadata()?;
    let identity = |metadata: &std::fs::Metadata| {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.mode(),
            metadata.nlink(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    };
    let invalid =
        || std::io::Error::other("bootstrap receipt is invalid or unsafe; retained for recovery");
    if !before.is_file()
        || before.nlink() != 1
        || before.mode() & 0o7777 != 0o600
        || before.uid() != std::fs::metadata("/proc/self")?.uid()
        || before.len() > 16 * 1024
    {
        return Err(invalid().into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    let named = std::fs::symlink_metadata(path)?;
    if !named.is_file()
        || identity(&before) != identity(&file.metadata()?)
        || identity(&before) != identity(&named)
        || bytes.len() as u64 != before.len()
    {
        return Err(invalid().into());
    }
    let shape: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if !shape.is_object()
        || shape
            .get("credentials")
            .and_then(serde_json::Value::as_array)
            .is_none_or(|entries| entries.iter().any(|entry| !entry.is_object()))
    {
        return Err(invalid().into());
    }
    let receipt: BootstrapCredentialFile = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if receipt.credentials.len() != 1 || receipt.generated_at.is_empty() {
        return Err(invalid().into());
    }
    let credential = receipt.credentials.into_iter().next().ok_or_else(invalid)?;
    if credential.login != BOOTSTRAP_ADMIN_LOGIN
        || credential.role != "admin"
        || credential.password.is_empty()
    {
        return Err(invalid().into());
    }
    file.sync_all()?;
    std::fs::File::open(path.parent().ok_or_else(invalid)?)?.sync_all()?;
    Ok(credential.password)
}

fn create_or_reset_bootstrap_admin(
    state: &WebState,
    password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    match state.core.get_account(BOOTSTRAP_ADMIN_LOGIN) {
        Ok(account) => {
            if account.role != UserRole::Admin {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{BOOTSTRAP_ADMIN_LOGIN} exists without admin role"),
                )));
            }
            state
                .core
                .reset_account_password(BOOTSTRAP_ADMIN_LOGIN, password)?;
            state
                .core
                .force_password_change(BOOTSTRAP_ADMIN_LOGIN, false)?;
        }
        Err(_) => {
            state
                .core
                .create_account(BOOTSTRAP_ADMIN_LOGIN, password, UserRole::Admin)?;
        }
    }
    Ok(())
}

fn secure_create(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
