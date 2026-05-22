use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub(crate) fn infer_remote_slug(repo_root: &Path) -> Result<Option<String>> {
    let Some(remote) = git_config_get(repo_root, "remote.origin.url")? else {
        return Ok(None);
    };
    Ok(parse_remote_slug(&remote))
}

pub(crate) fn parse_remote_slug(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (_, path) = rest.split_once(':')?;
        return normalize_slug(path);
    }
    if let Some((_, path)) = trimmed.split_once("://") {
        let mut parts = path.split('/').collect::<Vec<_>>();
        if parts.len() >= 3 {
            let repo = parts.pop()?;
            let owner = parts.pop()?;
            return normalize_slug(&format!("{owner}/{repo}"));
        }
    }
    normalize_slug(trimmed)
}

fn normalize_slug(value: &str) -> Option<String> {
    let mut parts = value
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let repo = parts.pop()?.trim_end_matches(".git");
    let owner = parts.pop()?;
    Some(format!("{owner}/{repo}"))
}

pub(crate) fn split_repo_slug(slug: &str) -> (String, String) {
    let mut parts = slug.split('/');
    let owner = parts.next().unwrap_or("repo-owner").to_string();
    let name = parts.next().unwrap_or("repo").to_string();
    (owner, name)
}

pub(crate) fn normalize_relative_path(path: &Path) -> Result<String> {
    if path.is_absolute() {
        bail!("path must be repo-relative: {}", path.display());
    }
    let value = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if value.is_empty() {
        bail!("path must not be empty");
    }
    Ok(value)
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(unix)]
pub(crate) fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(if executable { 0o755 } else { 0o644 });
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

pub(crate) fn git_config_get(repo_root: &Path, key: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "--get", key])
        .output()
        .with_context(|| format!("reading git config {key}"))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

pub(crate) fn run_git(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("running git {:?}", args))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
