//! Object validation using stock Git as the oracle.

use crate::command::{run_capture, run_with_stdin};
use crate::error::{GitdError, Result};
use crate::repo::Repository;
use std::collections::{BTreeMap, BTreeSet};

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

    /// Reject newly introduced raw Git blobs larger than `max_blob_bytes`.
    ///
    /// Git LFS pointer files are small Git blobs and pass this check. The large
    /// binary payload belongs in the repository-local LFS object store instead.
    pub fn reject_oversized_raw_blobs(
        &self,
        repo: &Repository,
        new_oids: &[String],
        max_blob_bytes: u64,
    ) -> Result<()> {
        let mut objects = BTreeSet::new();
        let mut paths = BTreeMap::new();
        for oid in new_oids {
            let out = run_capture(
                &self.git_bin,
                &["rev-list", "--objects", oid, "--not", "--all"],
                Some(&repo.path),
            )?;
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let mut parts = line.splitn(2, ' ');
                let Some(object_id) = parts.next().filter(|value| !value.is_empty()) else {
                    continue;
                };
                objects.insert(object_id.to_string());
                if let Some(path) = parts.next().filter(|value| !value.is_empty()) {
                    paths
                        .entry(object_id.to_string())
                        .or_insert_with(|| path.to_string());
                }
            }
        }
        if objects.is_empty() {
            return Ok(());
        }
        let mut input = String::new();
        for oid in &objects {
            input.push_str(oid);
            input.push('\n');
        }
        let out = run_with_stdin(
            &self.git_bin,
            &[
                "cat-file",
                "--batch-check=%(objecttype) %(objectsize) %(objectname)",
            ],
            input.as_bytes(),
            Some(&repo.path),
        )?;
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.split_whitespace();
            let Some(kind) = parts.next() else {
                continue;
            };
            let Some(size) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
                continue;
            };
            let Some(oid) = parts.next() else {
                continue;
            };
            if kind == "blob" && size > max_blob_bytes {
                let path = paths.get(oid).map(String::as_str).unwrap_or("<unknown>");
                return Err(GitdError::ProtectedRefDenied(format!(
                    "raw Git blob {path} is {size} bytes, exceeding GitHub's 100 MiB limit; track large model artifacts with Git LFS"
                )));
            }
        }
        Ok(())
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
