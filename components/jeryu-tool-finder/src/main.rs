//! jeryu-tool-finder: all-Rust cross-repo duplicate-code discovery.
//!
//! Pipeline: `scan` (family or whole-system) -> `dossier` (agent-readable
//! tool-candidate dossiers) -> `propose` (file a `[[tool]]` proposal + build
//! task into the sibling jeryu-tool registry). The scanning engine is the
//! `jeryu-codegraph` library — the same implementation the live API server
//! runs — so cluster ids, categories, and LOC numbers always agree.

mod dossier;
mod errors;
mod paths;
mod propose;
mod scan;
mod summary;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "jeryu-tool-finder",
    version,
    about = "Cross-repo duplicate-code discovery: scan, dossier, and propose shared tools"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for cross-repo repeated-code clusters and write dossiers/clusters.json.
    Scan(scan::ScanArgs),
    /// Enrich scanned clusters into agent-readable tool-candidate dossiers.
    Dossier(dossier::DossierArgs),
    /// Promote a dossier into a jeryu-tool registry proposal + build task.
    Propose(propose::ProposeArgs),
    /// Print the sibling jeryu-tool registry summary (delegates to its owner).
    Summary(summary::SummaryArgs),
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Commands::Scan(args) => scan::run(args),
        Commands::Dossier(args) => dossier::run(args),
        Commands::Propose(args) => propose::run(args),
        Commands::Summary(args) => summary::run(args),
    }
}
