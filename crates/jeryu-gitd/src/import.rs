//! Local Git import helpers for materializing imported repos into gitd storage.

use crate::error::{GitdError, Result};
use crate::mirror::MirrorService;
use crate::repo::{RepoId, RepoManager};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Classified local Git directory type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitDirKind {
    /// A normal worktree with a `.git` directory or gitdir file.
    Worktree,
    /// A bare repository directory.
    Bare,
}

/// Action taken while importing a Git directory into gitd storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitImportAction {
    /// The destination did not exist and was cloned as a mirror.
    Cloned,
    /// The destination already existed and was fetched from its origin.
    Fetched,
}

impl GitImportAction {
    /// Stable manifest string for operator-facing receipts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cloned => "cloned",
            Self::Fetched => "fetched",
        }
    }
}

/// One imported Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitImportEntry {
    /// Canonical source path.
    pub source_path: String,
    /// Owner used under gitd storage.
    pub owner: String,
    /// Repository name used under gitd storage.
    pub name: String,
    /// Whether the source path is bare.
    pub bare: bool,
    /// Materialized bare repository path under gitd storage.
    pub repo_path: String,
    /// Action performed.
    pub action: GitImportAction,
}

/// One skipped local import candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitImportSkip {
    /// Canonical source path.
    pub source_path: String,
    /// Owner that would have been used.
    pub owner: String,
    /// Repository name that would have been used.
    pub name: String,
    /// Reason the path was skipped.
    pub reason: String,
}

/// Result of importing local Git directories into gitd storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitImportReport {
    /// gitd storage root.
    pub storage_root: String,
    /// Imported repositories.
    pub imported: Vec<GitImportEntry>,
    /// Skipped candidates.
    pub skipped: Vec<GitImportSkip>,
}

/// Materializes local Git directories as gitd-managed bare mirrors.
#[derive(Clone, Debug)]
pub struct LocalGitImporter {
    manager: RepoManager,
}

impl LocalGitImporter {
    /// Create an importer using an existing repo manager.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self { manager }
    }

    /// Import local paths using either an explicit owner or one inferred per path.
    pub fn import_paths(&self, owner: Option<&str>, paths: &[PathBuf]) -> Result<GitImportReport> {
        let mut report = GitImportReport {
            storage_root: self.manager.config().storage_root.display().to_string(),
            imported: Vec::new(),
            skipped: Vec::new(),
        };
        for path in paths {
            let canonical = path.canonicalize()?;
            let repo_owner = owner
                .map(str::to_string)
                .unwrap_or_else(|| inferred_owner(&canonical));
            let name = repo_name(&canonical);
            let id = RepoId::new(&repo_owner, &name)?;
            match self.import_repo(&canonical, &id) {
                Ok(entry) => report.imported.push(entry),
                Err(GitdError::InvalidInput(reason)) if reason == "not a Git directory" => {
                    report.skipped.push(GitImportSkip {
                        source_path: canonical.display().to_string(),
                        owner: repo_owner,
                        name,
                        reason,
                    });
                }
                Err(err) => return Err(err),
            }
        }
        Ok(report)
    }

    /// Import one path into the provided repository id.
    pub fn import_repo(&self, source_path: &Path, id: &RepoId) -> Result<GitImportEntry> {
        let canonical = source_path.canonicalize()?;
        let Some(kind) = classify_git_dir(&canonical) else {
            return Err(GitdError::InvalidInput("not a Git directory".to_string()));
        };
        let existing = self.manager.open(id);
        let (repo, action) = match existing {
            Ok(repo) => {
                MirrorService::new(self.manager.clone()).mirror_fetch(&repo)?;
                let repo = self.manager.record_existing_bare(id)?;
                (repo, GitImportAction::Fetched)
            }
            Err(GitdError::RepoNotFound(_)) => {
                let remote = canonical.to_string_lossy().to_string();
                let repo = MirrorService::new(self.manager.clone()).mirror_clone(&remote, id)?;
                (repo, GitImportAction::Cloned)
            }
            Err(err) => return Err(err),
        };
        Ok(GitImportEntry {
            source_path: canonical.display().to_string(),
            owner: id.owner.clone(),
            name: id.name.clone(),
            bare: kind == GitDirKind::Bare,
            repo_path: repo.path.display().to_string(),
            action,
        })
    }
}

/// Classify a local path as a worktree or bare Git repository.
#[must_use]
pub fn classify_git_dir(path: &Path) -> Option<GitDirKind> {
    if path.join(".git").is_dir() || path.join(".git").is_file() {
        return Some(GitDirKind::Worktree);
    }
    if path.join("HEAD").is_file() && path.join("objects").is_dir() && path.join("refs").is_dir() {
        return Some(GitDirKind::Bare);
    }
    None
}

/// Infer a Jeryu repository name from a local path.
#[must_use]
pub fn repo_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .trim_end_matches(".git")
        .to_string()
}

/// Infer a Jeryu owner from a local path.
#[must_use]
pub fn inferred_owner(path: &Path) -> String {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.starts_with('.'))
        .unwrap_or("local")
        .to_string()
}
