//! Object validation using stock Git as the oracle.

use crate::command::run_capture;
use crate::error::Result;
use crate::repo::Repository;

/// Object integrity validator.
#[derive(Clone, Debug)]
pub struct ObjectFsck {
    git_bin: String,
}

impl ObjectFsck {
    /// Create an object validator.
    #[must_use]
    pub fn new(git_bin: impl Into<String>) -> Self {
        Self {
            git_bin: git_bin.into(),
        }
    }

    /// Run strict fsck. In receive-pack quarantine this observes Git's quarantine
    /// environment when invoked from the hook process.
    pub fn fsck(&self, repo: &Repository) -> Result<()> {
        run_capture(&self.git_bin, &["fsck", "--strict"], Some(&repo.path)).map(|_| ())
    }

    /// Check whether `old_oid` is an ancestor of `new_oid`.
    pub fn is_ancestor(&self, repo: &Repository, old_oid: &str, new_oid: &str) -> Result<bool> {
        let out = std::process::Command::new(&self.git_bin)
            .args(["merge-base", "--is-ancestor", old_oid, new_oid])
            .current_dir(&repo.path)
            .output()?;
        Ok(out.status.success())
    }
}
