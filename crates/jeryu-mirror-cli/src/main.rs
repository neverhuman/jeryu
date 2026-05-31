use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use jeryu_core::{CreateRepositoryRequest, ForgeCore, ForgeError};
use jeryu_gitd::import::{
    GitDirKind, LocalGitImporter, classify_git_dir, inferred_owner, repo_name,
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
            paths,
        } => {
            let data_dir = expand_tilde(data_dir);
            let manifest = import_local_git_dirs(&data_dir, owner.as_deref(), dry_run, &paths)?;
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
    paths: &[PathBuf],
) -> Result<LocalImportManifest> {
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("create data dir {}", data_dir.display()))?;
    let gitd_storage_root = data_dir.join("git").join("repos");
    let core = if dry_run {
        None
    } else {
        Some(ForgeCore::open_sqlite(data_dir.join("forge.sqlite"))?)
    };
    let gitd = if dry_run {
        None
    } else {
        Some(LocalGitImporter::new(RepoManager::new(GitdConfig::new(
            &gitd_storage_root,
        ))))
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
