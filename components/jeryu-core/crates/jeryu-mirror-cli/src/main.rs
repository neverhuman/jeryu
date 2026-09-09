use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use jeryu_core::{CreateRepositoryRequest, ForgeCore, ForgeError};
use jeryu_gitd::command::run_capture;
use jeryu_gitd::import::{
    GitDirKind, LocalGitImporter, classify_git_dir, forbidden_import_root, inferred_owner,
    repo_name,
};
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use jeryu_mirror::{
    InMemoryRestoreTarget, MirrorMode, MirrorSpec, RestoreOptions, archive_from_github_value,
    compare_archives, plan_git_mirror, plan_restore, read_bundle, verify_bundle, write_bundle,
};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(
    name = "jeryu_mirror",
    version,
    about = "Jeryu Phase 9 backup/import/restore CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    GithubBackup {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        bundle: PathBuf,
    },
    Verify {
        #[arg(long)]
        bundle: PathBuf,
    },
    RestorePlan {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long, default_value_t = true)]
        dry_run: bool,
    },
    Drift {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    MirrorPlan {
        #[arg(long)]
        name: String,
        #[arg(long)]
        source_url: String,
        #[arg(long)]
        destination_url: String,
        #[arg(long)]
        local_path: PathBuf,
        #[arg(long, value_enum, default_value_t = CliMirrorMode::FullSync)]
        mode: CliMirrorMode,
        #[arg(long, default_value_t = true)]
        prune: bool,
        #[arg(long, value_delimiter = ',')]
        allowed_refs: Vec<String>,
    },
    ImportLocal {
        #[arg(long, default_value = "~/.local/share/jeryu")]
        data_dir: PathBuf,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Re-import a path even when an existing managed mirror already
        /// points at it (skipped as a duplicate otherwise).
        #[arg(long, default_value_t = false)]
        force: bool,
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliMirrorMode {
    CloneIfMissing,
    FetchOnly,
    PushOnly,
    FullSync,
}

impl From<CliMirrorMode> for MirrorMode {
    fn from(value: CliMirrorMode) -> Self {
        match value {
            CliMirrorMode::CloneIfMissing => Self::CloneIfMissing,
            CliMirrorMode::FetchOnly => Self::FetchOnly,
            CliMirrorMode::PushOnly => Self::PushOnly,
            CliMirrorMode::FullSync => Self::FullSync,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::GithubBackup { input, bundle } => {
            let value = serde_json::from_str(
                &std::fs::read_to_string(&input)
                    .with_context(|| format!("read {}", input.display()))?,
            )?;
            let archive = archive_from_github_value(value)?;
            let manifest = write_bundle(&bundle, &archive)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Command::Verify { bundle } => {
            let verification = verify_bundle(&bundle)?;
            println!("{}", serde_json::to_string_pretty(&verification)?);
            if !verification.ok {
                std::process::exit(2);
            }
        }
        Command::RestorePlan { bundle, dry_run } => {
            let archive = read_bundle(&bundle)?;
            let mut target = InMemoryRestoreTarget::default();
            let report = plan_restore(
                &archive,
                &mut target,
                RestoreOptions {
                    dry_run,
                    ..RestoreOptions::default()
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.blockers.is_empty() {
                std::process::exit(3);
            }
        }
        Command::Drift { source, target } => {
            let source = read_bundle(source)?;
            let target = read_bundle(target)?;
            let report = compare_archives(&source, &target);
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.drift_detected {
                std::process::exit(4);
            }
        }
        Command::MirrorPlan {
            name,
            source_url,
            destination_url,
            local_path,
            mode,
            prune,
            allowed_refs,
        } => {
            let spec = MirrorSpec {
                name,
                source_url,
                destination_url,
                local_path,
                mode: mode.into(),
                prune,
                allowed_refs,
            };
            let plan = plan_git_mirror(&spec);
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Command::ImportLocal {
            data_dir,
            owner,
            dry_run,
            force,
            paths,
        } => {
            let data_dir = expand_tilde(data_dir);
            let manifest =
                import_local_git_dirs(&data_dir, owner.as_deref(), dry_run, force, &paths)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct LocalImportManifest {
    data_dir: String,
    gitd_storage_root: String,
    dry_run: bool,
    imported: Vec<LocalImportEntry>,
    skipped: Vec<LocalImportEntry>,
}

#[derive(Debug, Serialize)]
struct LocalImportEntry {
    path: String,
    owner: String,
    name: String,
    bare: bool,
    gitd_path: Option<String>,
    gitd_action: Option<String>,
    reason: Option<String>,
}

fn import_local_git_dirs(
    data_dir: &Path,
    owner: Option<&str>,
    dry_run: bool,
    force: bool,
    paths: &[PathBuf],
) -> Result<LocalImportManifest> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;
    let gitd_storage_root = data_dir.join("git");
    let gitd_config = GitdConfig::new(&gitd_storage_root);
    // Never import from inside jeryu's own state: the gitd storage root would
    // mirror managed mirrors into themselves, and the data dir / ~/.jeryu hold
    // daemon state, not source repositories.
    let mut guard_roots = vec![data_dir.to_path_buf()];
    if let Some(home) = std::env::var_os("HOME") {
        guard_roots.push(PathBuf::from(home).join(".jeryu"));
    }
    let core = if dry_run {
        None
    } else {
        Some(ForgeCore::open_sqlite(data_dir.join("forge.sqlite"))?)
    };
    let gitd = if dry_run {
        None
    } else {
        Some(
            LocalGitImporter::new(RepoManager::new(gitd_config.clone()))
                .with_guard_roots(guard_roots.clone()),
        )
    };
    let mut manifest = LocalImportManifest {
        data_dir: data_dir.display().to_string(),
        gitd_storage_root: gitd_storage_root.display().to_string(),
        dry_run,
        imported: Vec::new(),
        skipped: Vec::new(),
    };

    for path in paths {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalize {}", path.display()))?;
        if let Some(reason) = forbidden_import_root(&canonical, &gitd_storage_root, &guard_roots) {
            manifest.skipped.push(LocalImportEntry {
                path: canonical.display().to_string(),
                owner: owner.unwrap_or("local").to_string(),
                name: repo_name(&canonical),
                bare: false,
                gitd_path: None,
                gitd_action: None,
                reason: Some(reason),
            });
            continue;
        }
        let Some(kind) = classify_git_dir(&canonical) else {
            manifest.skipped.push(LocalImportEntry {
                path: canonical.display().to_string(),
                owner: owner.unwrap_or("local").to_string(),
                name: repo_name(&canonical),
                bare: false,
                gitd_path: None,
                gitd_action: None,
                reason: Some("not a Git directory".to_string()),
            });
            continue;
        };
        let repo_owner = owner
            .map(str::to_string)
            .unwrap_or_else(|| inferred_owner(&canonical));
        let name = repo_name(&canonical);
        if !force
            && let Some((existing_owner, existing_name)) =
                existing_import_of(&gitd_config.git_bin, &gitd_storage_root, &canonical)
        {
            manifest.skipped.push(LocalImportEntry {
                path: canonical.display().to_string(),
                owner: repo_owner,
                name,
                bare: kind == GitDirKind::Bare,
                gitd_path: None,
                gitd_action: None,
                reason: Some(format!(
                    "already imported as {existing_owner}/{existing_name} (use --force to re-import)"
                )),
            });
            continue;
        }
        let mut gitd_path = None;
        let mut gitd_action = None;
        if let Some(gitd) = &gitd {
            let entry = gitd.import_repo(&canonical, &RepoId::new(&repo_owner, &name)?)?;
            gitd_path = Some(entry.repo_path);
            gitd_action = Some(entry.action.as_str().to_string());
        }
        if let Some(core) = &core {
            match core.create_repository(
                &repo_owner,
                CreateRepositoryRequest {
                    name: name.clone(),
                    private: true,
                    description: Some(format!("imported from {}", canonical.display())),
                    default_branch: Some("main".to_string()),
                },
            ) {
                Ok(_) => {}
                Err(ForgeError::Conflict(_)) => {}
                Err(err) => return Err(err.into()),
            }
        }
        manifest.imported.push(LocalImportEntry {
            path: canonical.display().to_string(),
            owner: repo_owner,
            name,
            bare: kind == GitDirKind::Bare,
            gitd_path,
            gitd_action,
            reason: None,
        });
    }

    let manifest_path = data_dir.join("import-manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    Ok(manifest)
}

/// `(owner, name)` of an existing managed bare under `storage_root` whose
/// `remote.origin.url` canonicalizes to `canonical` — i.e. this local path was
/// already imported. Walks the two-level `owner/name.git` layout; anything
/// unreadable or non-local simply does not match.
fn existing_import_of(
    git_bin: &str,
    storage_root: &Path,
    canonical: &Path,
) -> Option<(String, String)> {
    let owners = std::fs::read_dir(storage_root).ok()?;
    for owner_entry in owners.flatten() {
        let owner_dir = owner_entry.path();
        if !owner_dir.is_dir() {
            continue;
        }
        let Ok(repos) = std::fs::read_dir(&owner_dir) else {
            continue;
        };
        for repo_entry in repos.flatten() {
            let bare = repo_entry.path();
            if !bare.join("HEAD").is_file() {
                continue;
            }
            let Ok(output) = run_capture(
                git_bin,
                &["config", "--get", "remote.origin.url"],
                Some(&bare),
            ) else {
                continue;
            };
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if url.is_empty() || url.contains("://") {
                continue;
            }
            let Ok(resolved) = Path::new(&url).canonicalize() else {
                continue;
            };
            if resolved == canonical {
                let owner = owner_entry.file_name().to_string_lossy().to_string();
                let name = repo_name(&bare);
                return Some((owner, name));
            }
        }
    }
    None
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        std::env::var_os("HOME").map_or(path, PathBuf::from)
    } else if let Some(rest) = raw.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or(path)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
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

    fn git_available() -> bool {
        StdCommand::new("git")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    /// Dedupe resolves an existing managed bare back to its source path via
    /// `remote.origin.url`, and only an exact canonical match counts.
    #[test]
    fn existing_import_of_matches_origin_url_by_canonical_path() {
        if !git_available() {
            return;
        }
        let base = temp_dir("jeryu-mirror-dedupe");
        let source = base.join("projects").join("foo");
        std::fs::create_dir_all(&source).expect("source dir");
        let other = base.join("projects").join("bar");
        std::fs::create_dir_all(&other).expect("other dir");
        let storage = base.join("git");
        let bare = storage.join("local").join("foo.git");
        std::fs::create_dir_all(bare.parent().expect("owner dir")).expect("create owner dir");
        run_capture("git", &["init", "--bare", &bare.to_string_lossy()], None).expect("init bare");
        let canonical_source = source.canonicalize().expect("canonical source");
        run_capture(
            "git",
            &[
                "config",
                "remote.origin.url",
                &canonical_source.to_string_lossy(),
            ],
            Some(&bare),
        )
        .expect("set origin url");

        assert_eq!(
            existing_import_of("git", &storage, &canonical_source),
            Some(("local".to_string(), "foo".to_string()))
        );
        assert_eq!(
            existing_import_of(
                "git",
                &storage,
                &other.canonicalize().expect("canonical other")
            ),
            None
        );

        let _ = std::fs::remove_dir_all(base);
    }

    /// The import flow refuses the jeryu data dir / storage root as sources
    /// (skipped with the guard reason) and dedupes already-imported paths
    /// unless forced. Dry-run keeps everything read-only.
    #[test]
    fn import_local_guards_and_dedupes() {
        if !git_available() {
            return;
        }
        let base = temp_dir("jeryu-mirror-import-guard");
        let data_dir = base.join("data");
        let storage = data_dir.join("git");
        let bare = storage.join("local").join("foo.git");
        std::fs::create_dir_all(bare.parent().expect("owner dir")).expect("create owner dir");
        run_capture("git", &["init", "--bare", &bare.to_string_lossy()], None).expect("init bare");
        let source = base.join("projects").join("foo");
        std::fs::create_dir_all(&source).expect("source dir");
        run_capture("git", &["init"], Some(&source)).expect("init source worktree");
        let canonical_source = source.canonicalize().expect("canonical source");
        run_capture(
            "git",
            &[
                "config",
                "remote.origin.url",
                &canonical_source.to_string_lossy(),
            ],
            Some(&bare),
        )
        .expect("set origin url");

        // A path inside the data dir is refused with the guard reason; the
        // already-imported source is skipped as a duplicate.
        let manifest = import_local_git_dirs(
            &data_dir,
            Some("local"),
            true,
            false,
            &[bare.clone(), source.clone()],
        )
        .expect("dry-run import");
        assert!(manifest.imported.is_empty());
        assert_eq!(manifest.skipped.len(), 2);
        let guard_reason = manifest.skipped[0].reason.as_deref().unwrap_or_default();
        assert!(guard_reason.contains("refusing import"), "{guard_reason}");
        let dedupe_reason = manifest.skipped[1].reason.as_deref().unwrap_or_default();
        assert!(
            dedupe_reason.contains("already imported as local/foo"),
            "{dedupe_reason}"
        );

        // --force bypasses the dedupe skip: the candidate flows through the
        // normal import path again (dry-run, so no side effects).
        let forced = import_local_git_dirs(&data_dir, Some("local"), true, true, &[source])
            .expect("forced dry-run import");
        assert!(forced.skipped.is_empty());
        assert_eq!(forced.imported.len(), 1);
        assert_eq!(forced.imported[0].name, "foo");

        let _ = std::fs::remove_dir_all(base);
    }
}
