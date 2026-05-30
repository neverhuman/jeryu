//! Repository creation and lookup.

use crate::command::run_capture;
use crate::config::GitdConfig;
use crate::error::{GitdError, Result};
use crate::path::{normalize_repo_name, safe_join, validate_segment};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

/// Logical repository identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RepoId {
    /// Repository owner or organization.
    pub owner: String,
    /// Repository name without `.git`.
    pub name: String,
}

impl RepoId {
    /// Create and validate an identifier.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Result<Self> {
        let owner = owner.into();
        let name = name.into();
        validate_segment(&owner, "owner")?;
        validate_segment(name.strip_suffix(".git").unwrap_or(&name), "repo")?;
        Ok(Self {
            owner,
            name: name.strip_suffix(".git").unwrap_or(&name).to_string(),
        })
    }

    /// Bare repository directory name.
    #[must_use]
    pub fn bare_name(&self) -> String {
        format!("{}.git", self.name)
    }
}

impl Display for RepoId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// Resolved repository on disk.
#[derive(Clone, Debug)]
pub struct Repository {
    /// Logical id.
    pub id: RepoId,
    /// Bare repository path.
    pub path: PathBuf,
}

/// Repository manager rooted at a storage directory.
#[derive(Clone, Debug)]
pub struct RepoManager {
    config: GitdConfig,
}

impl RepoManager {
    /// Create a manager.
    #[must_use]
    pub fn new(config: GitdConfig) -> Self {
        Self { config }
    }

    /// Access the active config.
    #[must_use]
    pub fn config(&self) -> &GitdConfig {
        &self.config
    }

    /// Resolve a repository id to a path.
    pub fn resolve(&self, id: &RepoId) -> Result<Repository> {
        let owner_root = safe_join(&self.config.storage_root, Path::new(&id.owner))?;
        let path = safe_join(&owner_root, Path::new(&id.bare_name()))?;
        Ok(Repository {
            id: id.clone(),
            path,
        })
    }

    /// Resolve owner and a repo name that may include `.git`.
    pub fn resolve_parts(&self, owner: &str, repo: &str) -> Result<Repository> {
        validate_segment(owner, "owner")?;
        let repo_git = normalize_repo_name(repo)?;
        let repo_name = repo_git.trim_end_matches(".git").to_string();
        self.resolve(&RepoId::new(owner, repo_name)?)
    }

    /// Create a bare repository and install Phase 1 metadata.
    pub fn create_bare(&self, id: &RepoId) -> Result<Repository> {
        let repo = self.resolve(id)?;
        if repo.path.exists() {
            return Err(GitdError::InvalidInput(format!(
                "repository already exists: {id}"
            )));
        }
        let parent = repo
            .path
            .parent()
            .ok_or_else(|| GitdError::InvalidPath("missing repository parent".to_string()))?;
        std::fs::create_dir_all(parent)?;
        let path_arg = repo.path.to_string_lossy().to_string();
        run_capture(&self.config.git_bin, &["init", "--bare", &path_arg], None)?;
        self.write_metadata(&repo)?;
        Ok(repo)
    }

    /// Open an existing repository.
    pub fn open(&self, id: &RepoId) -> Result<Repository> {
        let repo = self.resolve(id)?;
        if !repo.path.join("HEAD").is_file() {
            return Err(GitdError::RepoNotFound(repo.path));
        }
        Ok(repo)
    }

    /// Open a repository by URL path segments.
    pub fn open_parts(&self, owner: &str, repo: &str) -> Result<Repository> {
        let repo = self.resolve_parts(owner, repo)?;
        if !repo.path.join("HEAD").is_file() {
            return Err(GitdError::RepoNotFound(repo.path));
        }
        Ok(repo)
    }

    fn write_metadata(&self, repo: &Repository) -> Result<()> {
        let jf = repo.path.join("jeryu");
        std::fs::create_dir_all(&jf)?;
        std::fs::write(jf.join("phase"), b"phase1-git-server-core\n")?;
        std::fs::write(jf.join("repo-id"), repo.id.to_string())?;
        Ok(())
    }
}
