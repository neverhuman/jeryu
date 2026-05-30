//! Mirror clone/fetch/push helpers.

use crate::command::run_capture;
use crate::error::Result;
use crate::repo::{RepoId, RepoManager, Repository};

/// Mirror service.
#[derive(Clone, Debug)]
pub struct MirrorService {
    manager: RepoManager,
}

impl MirrorService {
    /// Create a mirror service.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self { manager }
    }

    /// Clone a remote URL as a bare mirror into JitForge storage.
    pub fn mirror_clone(&self, remote_url: &str, id: &RepoId) -> Result<Repository> {
        let repo = self.manager.resolve(id)?;
        if let Some(parent) = repo.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = repo.path.to_string_lossy().to_string();
        run_capture(
            &self.manager.config().git_bin,
            &["clone", "--mirror", remote_url, &path],
            None,
        )?;
        Ok(repo)
    }

    /// Fetch all refs from the configured origin mirror.
    pub fn mirror_fetch(&self, repo: &Repository) -> Result<()> {
        run_capture(
            &self.manager.config().git_bin,
            &["remote", "update", "--prune"],
            Some(&repo.path),
        )
        .map(|_| ())
    }

    /// Push all refs to a remote URL.
    pub fn mirror_push(&self, repo: &Repository, remote_url: &str) -> Result<()> {
        run_capture(
            &self.manager.config().git_bin,
            &["push", "--mirror", remote_url],
            Some(&repo.path),
        )
        .map(|_| ())
    }
}
