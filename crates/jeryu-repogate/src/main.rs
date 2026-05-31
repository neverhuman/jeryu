//! `jeryu-repogate` — governance/CI repo gates ported from `scripts/*.py`.
//!
//! Subcommands preserve the exact behavior, exit codes, and stdout signal
//! lines of the legacy Python scripts so existing CI lanes and downstream
//! tooling continue to parse them unchanged:
//!
//! * `score`          <- scripts/score-repo.py
//! * `security-scan`  <- scripts/security-scan.py
//! * `release-gate`   <- scripts/release-gate.py
//! * `gen-fixture`    <- scripts/generate-500-job-fixture.py

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use jeryu_repogate::{
    FIXTURE_RELATIVE_PATH, GateOutcome, run_affected_plan, run_gen_fixture, run_release_gate,
    run_score, run_security_scan,
};

#[derive(Debug, Parser)]
#[command(
    name = "jeryu-repogate",
    about = "Jeryu Phase 12 governance/CI repo gates"
)]
struct Cli {
    /// Repository root the gate operates against (defaults to the current directory).
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Repo score gate (port of scripts/score-repo.py).
    Score {
        /// Rewrite agent/baselines/main.repo-score.json with the computed result.
        #[arg(long)]
        write_baseline: bool,
    },
    /// Source security scan (port of scripts/security-scan.py).
    SecurityScan,
    /// Release readiness gate (port of scripts/release-gate.py).
    ReleaseGate,
    /// Emit the 500-job CI fixture (port of scripts/generate-500-job-fixture.py).
    GenFixture {
        /// Output path, relative to --root.
        #[arg(long, default_value = FIXTURE_RELATIVE_PATH)]
        out: String,
    },
    /// Emit the fast-lane affected-test plan.
    AffectedPlan {
        /// Base ref for committed changes.
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Output JSON path, relative to --root.
        #[arg(long, default_value = "target/ci-fast/affected-plan.json")]
        out: PathBuf,
        /// Worker count recorded in the plan.
        #[arg(long, default_value_t = 40)]
        workers: u32,
    },
    /// Verify hosted workflows remain thin wrappers around agent/ci-lanes.toml.
    CiLanesCheck,
    /// List commands from agent/ci-lanes.toml for shell wrappers.
    CiLanesList {
        /// Only include lanes marked full=true.
        #[arg(long)]
        full: bool,
        /// Emit JSON instead of tab-separated lines.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let root = cli.root.as_path();

    let outcome: GateOutcome = match cli.command {
        Command::Score { write_baseline } => run_score(root, write_baseline)?,
        Command::SecurityScan => run_security_scan(root)?,
        Command::ReleaseGate => run_release_gate(root),
        Command::GenFixture { out } => run_gen_fixture(root, &out)?,
        Command::AffectedPlan { base, out, workers } => {
            run_affected_plan(root, &base, &out, workers)?
        }
        Command::CiLanesCheck => jeryu_repogate::run_ci_lanes_check(root)?,
        Command::CiLanesList { full, json } => jeryu_repogate::run_ci_lanes_list(root, full, json)?,
    };

    for line in &outcome.stdout {
        println!("{line}");
    }

    Ok(ExitCode::from(u8::try_from(outcome.exit_code).unwrap_or(1)))
}
