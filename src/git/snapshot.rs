//! Owner: Git repository state snapshot
//! Proof: `cargo test -p jeryu -- git_snapshot`
//! Invariants: Snapshots are read-only and resilient to non-repository paths.

use anyhow::Result;
use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitSnapshot {
    pub repo_root: Option<String>,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub dirty: Option<bool>,
}

impl GitSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }
}

pub fn capture(cwd: &Path) -> Result<GitSnapshot> {
    let repo = match Repository::discover(cwd) {
        Ok(repo) => repo,
        Err(_) => return Ok(GitSnapshot::empty()),
    };

    let repo_root = repo.workdir().map(|path| path.display().to_string());
    let head = repo
        .head()
        .ok()
        .and_then(|reference| reference.target())
        .map(|oid| oid.to_string());
    let branch = repo
        .head()
        .ok()
        .and_then(|reference| reference.shorthand().map(str::to_string));

    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let dirty = repo.statuses(Some(&mut options)).ok().map(|statuses| {
        statuses
            .iter()
            .any(|entry| entry.status() != Status::CURRENT)
    });

    Ok(GitSnapshot {
        repo_root,
        head,
        branch,
        dirty,
    })
}

pub fn snapshot_or_empty(cwd: &Path) -> GitSnapshot {
    match capture(cwd) {
        Ok(snapshot) => snapshot,
        Err(_) => GitSnapshot::empty(),
    }
}
