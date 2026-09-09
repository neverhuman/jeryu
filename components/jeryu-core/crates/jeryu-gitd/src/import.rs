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
    /// Roots besides the storage root that imports must never source from
    /// (e.g. a daemon data dir). `$HOME/.jeryu` is always guarded on top.
    extra_guard_roots: Vec<PathBuf>,
}

impl LocalGitImporter {
    /// Create an importer using an existing repo manager.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self {
            manager,
            extra_guard_roots: Vec::new(),
        }
    }

    /// Add extra roots that imports must refuse to source from.
    #[must_use]
    pub fn with_guard_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.extra_guard_roots = roots;
        self
    }

    /// The configured extra roots plus `$HOME/.jeryu` (HOME resolved at call
    /// time so a daemon picking up a changed environment guards correctly).
    fn guard_roots(&self) -> Vec<PathBuf> {
        let mut roots = self.extra_guard_roots.clone();
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".jeryu"));
        }
        roots
    }

    /// Import local paths using either an explicit owner or one inferred per path.
    pub fn import_paths(&self, owner: Option<&str>, paths: &[PathBuf]) -> Result<GitImportReport> {
        let mut report = GitImportReport {
            storage_root: self.manager.config().storage_root.display().to_string(),
            imported: Vec::new(),
            skipped: Vec::new(),
        };
        let guard_roots = self.guard_roots();
        for path in paths {
            let canonical = path.canonicalize()?;
            let repo_owner = owner
                .map(str::to_string)
                .unwrap_or_else(|| inferred_owner(&canonical));
            let name = repo_name(&canonical);
            // Forbidden roots are a per-path skip (batch imports keep going),
            // recorded in the manifest with the refusal reason.
            if let Some(reason) = forbidden_import_root(
                &canonical,
                &self.manager.config().storage_root,
                &guard_roots,
            ) {
                report.skipped.push(GitImportSkip {
                    source_path: canonical.display().to_string(),
                    owner: repo_owner,
                    name,
                    reason,
                });
                continue;
            }
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
        if let Some(reason) = forbidden_import_root(
            &canonical,
            &self.manager.config().storage_root,
            &self.guard_roots(),
        ) {
            return Err(GitdError::InvalidInput(reason));
        }
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

/// Refusal reason when `canonical` lies under a root imports must never
/// source from: the gitd `storage_root` (importing managed storage recurses
/// the mirror into itself) or any of `extra_roots` (daemon data dirs,
/// `$HOME/.jeryu`, ...). Callers are responsible for passing an already
/// canonicalized candidate; the roots are canonicalized here when they exist
/// so a symlinked alias of a guarded root cannot dodge the check. Returns
/// `None` when the path is safe to import.
#[must_use]
pub fn forbidden_import_root(
    canonical: &Path,
    storage_root: &Path,
    extra_roots: &[PathBuf],
) -> Option<String> {
    let mut guarded: Vec<(PathBuf, &str)> = vec![(storage_root.to_path_buf(), "gitd storage root")];
    guarded.extend(
        extra_roots
            .iter()
            .map(|root| (root.clone(), "protected jeryu root")),
    );
    for (root, label) in guarded {
        let resolved = root.canonicalize().unwrap_or(root);
        if canonical.starts_with(&resolved) {
            return Some(format!(
                "refusing import: {} is inside the {label} {}",
                canonical.display(),
                resolved.display()
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitdConfig;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).expect("create temp dir");
        base
    }

    #[test]
    fn forbidden_import_root_guards_storage_and_extra_roots() {
        let base = temp_dir("jeryu-import-guard");
        let storage = base.join("storage");
        let data_dir = base.join("data");
        let elsewhere = base.join("projects").join("repo");
        std::fs::create_dir_all(storage.join("local/repo.git")).expect("storage layout");
        std::fs::create_dir_all(&data_dir).expect("data dir");
        std::fs::create_dir_all(&elsewhere).expect("project dir");

        let inside_storage = storage.join("local/repo.git").canonicalize().unwrap();
        let reason = forbidden_import_root(&inside_storage, &storage, &[])
            .expect("a managed bare must be refused");
        assert!(reason.contains("gitd storage root"), "{reason}");

        let inside_data = data_dir.canonicalize().unwrap().join("forge");
        let reason = forbidden_import_root(&inside_data, &storage, std::slice::from_ref(&data_dir))
            .expect("a path under an extra root must be refused");
        assert!(reason.contains("protected jeryu root"), "{reason}");

        let safe = elsewhere.canonicalize().unwrap();
        assert_eq!(forbidden_import_root(&safe, &storage, &[data_dir]), None);

        let _ = std::fs::remove_dir_all(base);
    }

    /// A symlinked alias of the storage root must not dodge the guard: the
    /// root is canonicalized before the prefix check, so a candidate reached
    /// through the REAL root is still refused when the configured root is the
    /// symlink (and vice versa, since the candidate is pre-canonicalized).
    #[cfg(unix)]
    #[test]
    fn forbidden_import_root_resolves_symlinked_roots() {
        let base = temp_dir("jeryu-import-guard-symlink");
        let storage = base.join("storage");
        std::fs::create_dir_all(storage.join("local/repo.git")).expect("storage layout");
        let link = base.join("storage-link");
        std::os::unix::fs::symlink(&storage, &link).expect("symlink storage root");

        let candidate = storage.join("local/repo.git").canonicalize().unwrap();
        assert!(
            forbidden_import_root(&candidate, &link, &[]).is_some(),
            "a symlinked storage root must still guard its real contents"
        );

        let _ = std::fs::remove_dir_all(base);
    }

    /// Batch imports record guard refusals in the skipped manifest with the
    /// reason instead of failing the whole run.
    #[test]
    fn import_paths_skips_guarded_roots_with_reason() {
        let base = temp_dir("jeryu-import-guard-batch");
        let storage = base.join("storage");
        let managed = storage.join("local").join("managed.git");
        std::fs::create_dir_all(&managed).expect("managed bare dir");
        let data_dir = base.join("data");
        let in_data = data_dir.join("cache.git");
        std::fs::create_dir_all(&in_data).expect("data layout");

        let importer = LocalGitImporter::new(RepoManager::new(GitdConfig::new(&storage)))
            .with_guard_roots(vec![data_dir]);
        let report = importer
            .import_paths(Some("local"), &[managed, in_data])
            .expect("guard refusals must not abort the batch");
        assert!(report.imported.is_empty());
        assert_eq!(report.skipped.len(), 2);
        assert!(report.skipped[0].reason.contains("gitd storage root"));
        assert!(report.skipped[1].reason.contains("protected jeryu root"));

        let _ = std::fs::remove_dir_all(base);
    }
}
