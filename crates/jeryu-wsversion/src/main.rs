//! `jeryu-wsversion` — auto-versioning engine CLI.
//!
//! Subcommands (mirroring the `jeryu-repogate` CLI shape: global `--root`,
//! one subcommand per gate):
//!
//! * `decide` — dry-run: print the computed next version + reason. Pure, never
//!   writes; used in PR/CI dry-runs. Exit 0.
//! * `apply` — write the bumped `[workspace.package].version` and roll
//!   `CHANGELOG.md`. Merge-queue / single-writer only. The binary never
//!   git-commits or pushes.
//! * `inherit-guard` — fail (exit 1) if any workspace member pins a literal
//!   version instead of `version.workspace = true`.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use jeryu_wsversion::{GateOutcome, assert_members_inherit, commits_in_range, decide};

#[derive(Debug, Parser)]
#[command(
    name = "jeryu-wsversion",
    about = "Jeryu workspace auto-versioning engine (conventional-commit -> version + CHANGELOG)"
)]
struct Cli {
    /// Repository root the engine operates against (defaults to the current directory).
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Dry-run: compute and print the next version + reason (no writes).
    Decide {
        /// Commit range to classify (e.g. `origin/main..HEAD`).
        #[arg(long, default_value = "origin/main..HEAD")]
        range: String,
        /// Emit the Decision as JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Apply the decision: rewrite Cargo.toml + CHANGELOG.md (single-writer only).
    Apply {
        /// Commit range to classify (e.g. `origin/main..HEAD`).
        #[arg(long, default_value = "origin/main..HEAD")]
        range: String,
    },
    /// Fail if any workspace member does not inherit the workspace version.
    InheritGuard,
}

fn run_decide(root: &std::path::Path, range: &str, json: bool) -> Result<GateOutcome> {
    let decision = decide(root, range)?;
    let stdout = if json {
        vec![serde_json::to_string(&decision)?]
    } else {
        vec![
            format!(
                "version: {} -> {} ({})",
                decision.from, decision.to, decision.bump
            ),
            format!("reason: {}", decision.reason),
            format!("commits: {}", decision.commits),
        ]
    };
    Ok(GateOutcome::ok(stdout))
}

fn run_apply(root: &std::path::Path, range: &str) -> Result<GateOutcome> {
    let decision = decide(root, range)?;
    let commits = commits_in_range(root, range)?;
    jeryu_wsversion::apply(root, &decision, &commits)?;
    let stdout = if decision.skipped || decision.to == decision.from {
        vec![format!(
            "no version change ({}); Cargo.toml + CHANGELOG.md untouched",
            decision.reason
        )]
    } else {
        vec![
            format!("applied version {} -> {}", decision.from, decision.to),
            "wrote: Cargo.toml [workspace.package].version, CHANGELOG.md".to_string(),
        ]
    };
    Ok(GateOutcome::ok(stdout))
}

fn run_inherit_guard(root: &std::path::Path) -> Result<GateOutcome> {
    let offenders = assert_members_inherit(root)?;
    if offenders.is_empty() {
        Ok(GateOutcome::ok(vec![
            "inherit-guard: all workspace members inherit version.workspace = true".to_string(),
        ]))
    } else {
        let mut stdout = vec![format!(
            "inherit-guard: {} member(s) do NOT inherit the workspace version:",
            offenders.len()
        )];
        for member in &offenders {
            stdout.push(format!("  - {member}: set version.workspace = true"));
        }
        Ok(GateOutcome::fail(stdout))
    }
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let root = cli.root.as_path();

    let outcome = match cli.command {
        Command::Decide { range, json } => run_decide(root, &range, json)?,
        Command::Apply { range } => run_apply(root, &range)?,
        Command::InheritGuard => run_inherit_guard(root)?,
    };

    for line in &outcome.stdout {
        println!("{line}");
    }
    Ok(ExitCode::from(u8::try_from(outcome.exit_code).unwrap_or(1)))
}
