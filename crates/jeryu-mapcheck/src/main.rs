//! `jeryu-mapcheck` — jankurai governance/CI gates.
//!
//! A single binary that replaces the legacy `scripts/check-*.py` validators.
//! Every subcommand reproduces the validation, stdout signal lines, and exit
//! codes of its Python predecessor (exit 0 on pass, non-zero on fail).

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use jeryu_mapcheck::{GateReport, agent_maps, docs, fixtures, generated_zones, owner_test_map};

#[derive(Parser)]
#[command(
    name = "jeryu-mapcheck",
    about = "Jankurai map/zone/fixture/doc governance gates (Rust port of scripts/check-*.py)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Port of scripts/check-owner-test-map.py: required glob roots + entry shape.
    OwnerTestMap(MapArgs),
    /// Port of scripts/check-agent-maps.py: required roots + entry shape (required_reviews >= 1).
    AgentMaps(MapArgs),
    /// Port of scripts/check-generated-zones.py: required generated zones + generators.
    GeneratedZones(ZonesArgs),
    /// Port of scripts/check-fixtures.py: every tests/fixtures/**/*.json parses.
    Fixtures(FixturesArgs),
    /// Port of scripts/check-docs.py: required docs exist and contain required markers.
    Docs(DocsArgs),
}

#[derive(Args)]
struct MapArgs {
    /// Path to the owner map JSON (default: agent/owner-map.json).
    #[arg(long, default_value = "agent/owner-map.json")]
    owner_map: PathBuf,
    /// Path to the test map JSON (default: agent/test-map.json).
    #[arg(long, default_value = "agent/test-map.json")]
    test_map: PathBuf,
}

#[derive(Args)]
struct ZonesArgs {
    /// Path to the generated-zones TOML (default: agent/generated-zones.toml).
    #[arg(long, default_value = "agent/generated-zones.toml")]
    zones: PathBuf,
}

#[derive(Args)]
struct FixturesArgs {
    /// Root directory to scan for *.json fixtures (default: tests/fixtures).
    #[arg(long, default_value = "tests/fixtures")]
    dir: PathBuf,
}

#[derive(Args)]
struct DocsArgs {
    /// Base directory the required docs are resolved against (default: current dir).
    #[arg(long, default_value = ".")]
    base: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(report) => emit(&report),
        Err(err) => {
            // Read/parse failures: the Python equivalents raised an uncaught
            // exception and exited non-zero. Mirror that with a stderr message
            // and a failing exit code.
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<GateReport> {
    let cli = Cli::parse();
    match cli.command {
        Command::OwnerTestMap(a) => owner_test_map(&a.owner_map, &a.test_map),
        Command::AgentMaps(a) => agent_maps(&a.owner_map, &a.test_map),
        Command::GeneratedZones(a) => generated_zones(&a.zones),
        Command::Fixtures(a) => fixtures(&a.dir),
        Command::Docs(a) => docs(&a.base),
    }
}

fn emit(report: &GateReport) -> ExitCode {
    for line in &report.lines {
        println!("{line}");
    }
    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
