//! gitd-backed [`RepoMaterializer`] for the unified `jeryu serve`.
//!
//! When the forge core creates a repository, this materializer also creates the
//! matching bare git repository on disk so clone URLs resolve and `git
//! clone`/`push` work over the mounted smart-HTTP transport. It is idempotent:
//! an already-present bare repository is success, not an error.

use std::sync::Arc;

use jeryu_core::{ForgeError, RepoMaterializer, Result};
use jeryu_gitd::{RepoId, RepoManager};

/// Creates bare git repositories on disk via a shared [`RepoManager`].
#[derive(Debug)]
pub struct GitMaterializer {
    manager: Arc<RepoManager>,
}

impl GitMaterializer {
    /// Wrap a shared [`RepoManager`].
    #[must_use]
    pub fn new(manager: Arc<RepoManager>) -> Self {
        Self { manager }
    }
}

impl RepoMaterializer for GitMaterializer {
    fn materialize(&self, owner: &str, name: &str, _default_branch: &str) -> Result<()> {
        let id = RepoId::new(owner, name).map_err(|err| {
            ForgeError::Validation(format!("invalid repository id {owner}/{name}: {err}"))
        })?;
        // Idempotent: a bare repository that already exists is success.
        let repo = match self.manager.open(&id) {
            Ok(repo) => repo,
            Err(_) => self.manager.create_bare(&id).map_err(|err| {
                ForgeError::Storage(format!("create bare repository {owner}/{name}: {err}"))
            })?,
        };
        self.manager
            .install_pre_receive_hook(&repo)
            .map_err(|err| {
                ForgeError::Storage(format!(
                    "install pre-receive hook for {owner}/{name}: {err}"
                ))
            })?;
        Ok(())
    }
}
