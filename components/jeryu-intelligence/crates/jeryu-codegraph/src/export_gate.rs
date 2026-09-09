//! Export slice gate.
//!
//! Runs `git diff --name-only <base>..<head>` against a bare repository, then
//! validates every changed (repo-relative) path against an allow-listed
//! `Slice`. The git invocation is isolated behind an inner function so tests can
//! supply the diff lines directly without a real repository.

use std::path::Path;
use std::process::Command;

use crate::error::{CodeGraphError, Result};
use crate::slice::{OutOfSlice, Slice};

/// Returned when an export is rejected: either one or more changed paths fall
/// outside the export slice, or the git diff could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceDenied {
    /// The repo-relative paths that were not permitted by the slice. Empty when
    /// the denial is due to a git invocation failure (see `git_error`).
    pub out_of_slice_paths: Vec<String>,
    /// Set when the denial is caused by a git invocation failure rather than a
    /// slice violation.
    pub git_error: Option<String>,
}

impl From<OutOfSlice> for SliceDenied {
    fn from(value: OutOfSlice) -> Self {
        Self {
            out_of_slice_paths: value.out_of_slice_paths,
            git_error: None,
        }
    }
}

impl From<CodeGraphError> for SliceDenied {
    fn from(value: CodeGraphError) -> Self {
        Self {
            out_of_slice_paths: Vec::new(),
            git_error: Some(value.to_string()),
        }
    }
}

/// Validates that the diff between `base_sha` and `head_sha` (in the bare repo
/// at `bare_repo`, using `git_bin`) only touches paths permitted by
/// `allowed_prefixes`.
///
/// On success returns the validated changed files (repo-relative). On a slice
/// violation returns `Err(SliceDenied { out_of_slice_paths, .. })`; a git
/// invocation failure is reported as `Err(SliceDenied { git_error: Some(..), ..
/// })`.
///
/// The git invocation is isolated in an inner function so tests exercise the
/// slice logic via [`enforce_export_slice_from_diff`] without a real repo.
pub fn enforce_export_slice(
    base_sha: &str,
    head_sha: &str,
    git_bin: &str,
    bare_repo: &Path,
    allowed_prefixes: &[String],
) -> std::result::Result<Vec<String>, SliceDenied> {
    let diff_lines = run_git_diff(git_bin, bare_repo, base_sha, head_sha)?;
    enforce_export_slice_from_diff(&diff_lines, allowed_prefixes)
}

/// Core slice logic, decoupled from git so it is deterministically testable.
///
/// Takes the already-parsed diff lines (repo-relative path per line) and the
/// allowed prefixes, and applies the corrected `Slice` predicate.
pub fn enforce_export_slice_from_diff(
    diff_lines: &[String],
    allowed_prefixes: &[String],
) -> std::result::Result<Vec<String>, SliceDenied> {
    let changed_files: Vec<String> = diff_lines
        .iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    let slice = Slice::new(allowed_prefixes.iter().cloned());
    match slice.slice_permits(&changed_files) {
        Ok(()) => Ok(changed_files),
        Err(out) => Err(out.into()),
    }
}

/// Runs `git --git-dir=<bare> diff --name-only <base>..<head>` and returns the
/// output as repo-relative lines.
fn run_git_diff(
    git_bin: &str,
    bare_repo: &Path,
    base_sha: &str,
    head_sha: &str,
) -> Result<Vec<String>> {
    let output = Command::new(git_bin)
        .arg(format!("--git-dir={}", bare_repo.display()))
        .arg("diff")
        .arg("--name-only")
        .arg(format!("{base_sha}..{head_sha}"))
        .output()
        .map_err(|e| CodeGraphError::Storage(format!("git invocation failed: {e}")))?;
    if !output.status.success() {
        return Err(CodeGraphError::Storage(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_when_path_outside_slice() {
        let diff = vec!["crates/jeryu-core/x.rs".to_string()];
        let allowed = vec!["crates/jeryu-codegraph".to_string()];
        let result = enforce_export_slice_from_diff(&diff, &allowed);
        let denied = result.expect_err("must be denied");
        assert_eq!(denied.out_of_slice_paths, vec!["crates/jeryu-core/x.rs"]);
    }

    #[test]
    fn allow_when_all_in_slice() {
        let diff = vec![
            "crates/jeryu-codegraph/src/lib.rs".to_string(),
            "crates/jeryu-codegraph/Cargo.toml".to_string(),
        ];
        let allowed = vec!["crates/jeryu-codegraph".to_string()];
        let ok = enforce_export_slice_from_diff(&diff, &allowed).expect("must allow");
        assert_eq!(ok, diff);
    }

    #[test]
    fn empty_allowed_denies() {
        let diff = vec!["crates/jeryu-codegraph/src/lib.rs".to_string()];
        let result = enforce_export_slice_from_diff(&diff, &[]);
        assert!(result.is_err());
    }
}
