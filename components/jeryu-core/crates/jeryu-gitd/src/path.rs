//! Safe path handling.

use crate::error::{GitdError, Result};
use std::path::{Component, Path, PathBuf};

/// Validate one logical path segment such as an owner or repository name.
pub fn validate_segment(name: &str, field: &str) -> Result<()> {
    if name.is_empty() {
        return Err(GitdError::InvalidPath(format!("{field} cannot be empty")));
    }
    if name == "." || name == ".." {
        return Err(GitdError::InvalidPath(format!("{field} cannot be {name}")));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(GitdError::InvalidPath(format!(
            "{field} contains a path separator or NUL"
        )));
    }
    if name.starts_with('-') {
        return Err(GitdError::InvalidPath(format!(
            "{field} cannot start with '-'"
        )));
    }
    Ok(())
}

/// Join path components under `root` without permitting traversal.
pub fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::from(root);
    for component in relative.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(GitdError::InvalidPath(format!(
                    "path escapes root: {}",
                    relative.display()
                )));
            }
        }
    }
    Ok(out)
}

/// Normalize repository names accepted in URLs and SSH commands.
pub fn normalize_repo_name(repo: &str) -> Result<String> {
    let repo = repo.trim_matches('/');
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    validate_segment(repo, "repo")?;
    Ok(format!("{repo}.git"))
}
