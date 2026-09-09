//! Create-once bootstrap account and credential-receipt handling.

use super::*;

#[derive(Debug, Serialize)]
struct BootstrapCredential {
    login: String,
    role: String,
    password: String,
}

#[derive(Debug, Serialize)]
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
    let mut credentials = Vec::new();
    if let Some(password) = admin_password {
        create_or_reset_bootstrap_admin(state, password)?;
    }
    if admin_password.is_none() && state.core.get_account(BOOTSTRAP_ADMIN_LOGIN).is_err() {
        let password = state.core.generate_one_time_password()?;
        state
            .core
            .create_temporary_account(BOOTSTRAP_ADMIN_LOGIN, &password, UserRole::Admin)?;
        credentials.push(BootstrapCredential {
            login: BOOTSTRAP_ADMIN_LOGIN.to_string(),
            role: bootstrap_role_name(&UserRole::Admin).to_string(),
            password,
        });
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

    if credentials.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(data_dir)?;
    let receipt = BootstrapCredentialFile {
        generated_at: chrono_like_now(),
        credentials,
    };
    let path = next_bootstrap_receipt_path(data_dir);
    let json = serde_json::to_vec_pretty(&receipt)?;
    let mut file = secure_create(&path)?;
    file.write_all(&json)?;
    file.write_all(b"\n")?;
    Ok(())
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

fn bootstrap_role_name(role: &UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::User => "user",
    }
}

fn next_bootstrap_receipt_path(data_dir: &Path) -> PathBuf {
    let primary = data_dir.join("bootstrap-credentials.json");
    if !primary.exists() {
        return primary;
    }
    data_dir.join(format!(
        "bootstrap-credentials-{}.json",
        jeryu_runner_core::receipt::now_ms()
    ))
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
