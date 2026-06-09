use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::fs_error;
use crate::{AgentAuthError, AgentToolKind, AuthFileReceipt};

pub(crate) fn auth_dir(data_home: &Path, tool: AgentToolKind) -> PathBuf {
    data_home.join("agent-auth").join(tool.as_str())
}

pub(crate) fn create_private_dir(path: &Path) -> Result<(), AgentAuthError> {
    std::fs::create_dir_all(path).map_err(fs_error)?;
    set_dir_private(path)?;
    Ok(())
}

pub(crate) fn copy_imported_files(
    files: &[AuthFileReceipt],
    target_dir: &Path,
) -> Result<Vec<AuthFileReceipt>, AgentAuthError> {
    let mut copied = Vec::new();
    for file in files {
        let name = file.path.file_name().ok_or_else(|| {
            AgentAuthError::new(
                "agent_auth_invalid_path",
                "materialize imported auth",
                format!(
                    "imported auth path '{}' has no file name",
                    file.path.display()
                ),
                &["remove the malformed auth file and re-import"],
                "docs/testing.md#workcells",
                "rerun cargo test -p jeryu-agent-auth --jobs 40",
            )
        })?;
        copied.push(copy_private_file(&file.path, &target_dir.join(name))?);
    }
    Ok(copied)
}

pub(crate) fn copy_private_file(
    source: &Path,
    target: &Path,
) -> Result<AuthFileReceipt, AgentAuthError> {
    let bytes = std::fs::read(source).map_err(fs_error)?;
    write_private_file(target, &bytes)
}

pub(crate) fn write_private_file(
    target: &Path,
    bytes: &[u8],
) -> Result<AuthFileReceipt, AgentAuthError> {
    if let Some(parent) = target.parent() {
        create_private_dir(parent)?;
    }
    std::fs::write(target, bytes).map_err(fs_error)?;
    set_file_private(target)?;
    receipt_for_file(target)
}

/// Write `bytes` to `target` so readers never observe a partial credential: the
/// content lands in a sibling 0600 file and is renamed over `target`.
pub(crate) fn write_private_file_atomic(
    target: &Path,
    bytes: &[u8],
) -> Result<AuthFileReceipt, AgentAuthError> {
    if let Some(parent) = target.parent() {
        create_private_dir(parent)?;
    }
    let pending = pending_sibling(target);
    std::fs::write(&pending, bytes).map_err(fs_error)?;
    set_file_private(&pending)?;
    std::fs::rename(&pending, target).map_err(fs_error)?;
    receipt_for_file(target)
}

fn pending_sibling(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".pending");
    target.with_file_name(name)
}

pub(crate) fn receipts_for_dir(path: &Path) -> Result<Vec<AuthFileReceipt>, AgentAuthError> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(path).map_err(fs_error)? {
        let entry = entry.map_err(fs_error)?;
        if entry.file_type().map_err(fs_error)?.is_file() {
            files.push(receipt_for_file(&entry.path())?);
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn receipt_for_file(path: &Path) -> Result<AuthFileReceipt, AgentAuthError> {
    let bytes = std::fs::read(path).map_err(fs_error)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(AuthFileReceipt {
        path: path.to_path_buf(),
        digest: format!("sha256:{}", hex::encode(hasher.finalize())),
        mode: file_mode(path)?,
    })
}

#[cfg(unix)]
fn set_file_private(path: &Path) -> Result<(), AgentAuthError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).map_err(fs_error)?.permissions();
    perms.set_mode(0o600);
    std::fs::set_permissions(path, perms).map_err(fs_error)
}

#[cfg(not(unix))]
fn set_file_private(_path: &Path) -> Result<(), AgentAuthError> {
    Ok(())
}

#[cfg(unix)]
fn set_dir_private(path: &Path) -> Result<(), AgentAuthError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).map_err(fs_error)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(path, perms).map_err(fs_error)
}

#[cfg(not(unix))]
fn set_dir_private(_path: &Path) -> Result<(), AgentAuthError> {
    Ok(())
}

#[cfg(unix)]
fn file_mode(path: &Path) -> Result<String, AgentAuthError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)
        .map_err(fs_error)?
        .permissions()
        .mode()
        & 0o777;
    Ok(format!("{mode:04o}"))
}

#[cfg(not(unix))]
fn file_mode(_path: &Path) -> Result<String, AgentAuthError> {
    Ok("platform-default".to_string())
}
