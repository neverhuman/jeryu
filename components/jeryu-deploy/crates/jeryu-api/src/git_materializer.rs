//! UUID-bound Git initialization, staged before atomic publication.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use anyhow::{Result, ensure};
use jeryu_core::{ForgeError, RepoMaterializer, Repository};
use jeryu_gitd::{RepoId, RepoManager};
use serde::{Deserialize, Serialize};

pub(crate) mod storage;
use storage::Directory;
pub(crate) use storage::sync_tree as sync_repository;

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    id: uuid::Uuid,
    owner: String,
    name: String,
    branch: String,
}

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

    pub(crate) fn verify_published(&self, repository: &Repository) -> Result<()> {
        let path = self
            .manager
            .resolve_parts(&repository.owner, &repository.name)?
            .path;
        ensure!(
            std::fs::symlink_metadata(&path)?.is_dir(),
            "published Git storage is missing or replaced"
        );
        let directory = Directory::open(&path)?;
        let identity: Identity = directory.read("jeryu-creation.json")?;
        ensure!(
            identity.id == repository.id
                && identity.owner == repository.owner
                && identity.name == repository.name,
            "published Git storage has a different identity"
        );
        ensure!(
            self.git(&path, &["rev-parse", "--is-bare-repository"])? == "true",
            "published Git storage is incomplete"
        );
        directory.check()?;
        Ok(())
    }

    pub(crate) fn resume(&self, repository: &Repository) -> Result<()> {
        self.validate(
            &repository.owner,
            &repository.name,
            &repository.default_branch,
        )?;
        ensure!(
            !repository.id.is_nil(),
            "missing repository creation identity"
        );
        let id = RepoId::new(&repository.owner, &repository.name)?;
        let identity = Identity {
            id: repository.id,
            owner: repository.owner.clone(),
            name: repository.name.clone(),
            branch: repository.default_branch.clone(),
        };
        let root = Directory::open(&self.manager.config().storage_root)?;
        let journal = Directory::open(&root.path.join(".jeryu-creation"))?;
        let key = repository.id.to_string();
        let _lock = journal.lock(&format!("{key}.lock"))?;
        let owner = root.child(&id.owner)?;
        let destination = id.bare_name();
        if owner.exists(&destination)? {
            let existing = owner.child(&destination)?;
            ensure!(
                existing.read::<Identity>("jeryu-creation.json")? == identity,
                "repository storage belongs to another creation"
            );
            // Never initialize, reset HEAD, or rewrite hooks on a published repo.
            ensure!(
                self.git(&existing.path, &["rev-parse", "--is-bare-repository"])? == "true",
                "published repository is incomplete"
            );
            owner.file.sync_all()?;
            root.check()?;
            return Ok(());
        }
        let stage = journal.child(&key)?;
        if stage.exists("intent.json")? {
            ensure!(
                stage.read::<Identity>("intent.json")? == identity,
                "staging identity mismatch"
            );
        } else {
            ensure!(
                !stage.exists(&id.owner)?,
                "unbound staging repository retained for recovery"
            );
            stage.write("intent.json", &identity, false)?;
        }
        let stage_owner = stage.child(&id.owner)?;
        let bare = stage_owner.child(&destination)?;
        storage::sync_tree(&bare.path)?;
        self.git(
            &bare.path,
            &[
                "init",
                "--bare",
                "--shared=0600",
                "--template=",
                "--initial-branch",
                &identity.branch,
                ".",
            ],
        )?;
        self.git(
            &bare.path,
            &[
                "symbolic-ref",
                "HEAD",
                &format!("refs/heads/{}", identity.branch),
            ],
        )?;
        let mut config = self.manager.config().clone();
        config.storage_root = stage.path.clone();
        let manager = RepoManager::new(config);
        let staged_repo = manager.record_existing_bare(&id)?;
        manager.install_pre_receive_hook(&staged_repo)?;
        if bare.exists("jeryu-creation.json")? {
            ensure!(
                bare.read::<Identity>("jeryu-creation.json")? == identity,
                "staged repository identity mismatch"
            );
        } else {
            bare.write("jeryu-creation.json", &identity, false)?;
        }
        storage::sync_tree(&bare.path)?;
        for directory in [&root, &journal, &stage, &stage_owner, &bare, &owner] {
            directory.check()?;
        }
        rustix::fs::renameat_with(
            &stage_owner.file,
            &destination,
            &owner.file,
            &destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )?;
        // Both sides of the rename must be durable before Core can mark success.
        owner.file.sync_all()?;
        stage_owner.file.sync_all()?;
        owner.check()?;
        stage_owner.check()?;
        root.check()?;
        Ok(())
    }

    fn git(&self, path: &Path, args: &[&str]) -> Result<String> {
        let output = self.command().current_dir(path).args(args).output()?;
        ensure!(
            output.status.success(),
            "Git creation command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.manager.config().git_bin);
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0");
        command
    }
}

impl RepoMaterializer for GitMaterializer {
    fn validate(&self, owner: &str, name: &str, default_branch: &str) -> jeryu_core::Result<()> {
        RepoId::new(owner, name).map_err(|err| {
            ForgeError::Validation(format!("invalid repository id {owner}/{name}: {err}"))
        })?;
        let output = self
            .command()
            .args(["check-ref-format", &format!("refs/heads/{default_branch}")])
            .output()
            .map_err(|err| ForgeError::Storage(err.to_string()))?;
        if name.ends_with(".git") || default_branch.starts_with('-') || !output.status.success() {
            return Err(ForgeError::Validation(
                "invalid repository name or default branch".into(),
            ));
        }
        Ok(())
    }

    fn materialize(
        &self,
        _owner: &str,
        _name: &str,
        _default_branch: &str,
    ) -> jeryu_core::Result<()> {
        Err(ForgeError::Validation(
            "Git creation requires Core's durable repository identity".into(),
        ))
    }

    fn materialize_repository(&self, repository: &Repository) -> jeryu_core::Result<()> {
        self.resume(repository)
            .map_err(|error| ForgeError::Storage(format!("repository creation pending: {error:#}")))
    }
}
