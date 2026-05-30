use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use jeryu_mirror::{
    InMemoryRestoreTarget, MirrorMode, MirrorSpec, RestoreOptions, archive_from_github_value,
    compare_archives, plan_git_mirror, plan_restore, read_bundle, verify_bundle, write_bundle,
};

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
    }
    Ok(())
}
