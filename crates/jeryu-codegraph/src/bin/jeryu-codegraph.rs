//! `jeryu-codegraph` CLI: index the workspace, query impact, and check slices.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jeryu_codegraph::{
    CodeGraph, CodeGraphStore, QueryOptions, RepoIdentity, Slice, default_db_path,
};
use jeryu_rustjet::WorkspaceGraph;
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "jeryu-codegraph",
    about = "Additive workspace code graph: symbol index, impact, and export slice gate"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index the workspace and persist the code graph.
    Index {
        /// Workspace root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Report impact of changed repo-relative paths.
    Impact {
        /// Workspace root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Changed repo-relative paths.
        paths: Vec<String>,
    },
    /// Check changed files against an export slice.
    SliceCheck {
        /// Comma-separated allowed prefixes.
        prefixes_csv: String,
        /// Comma-separated changed repo-relative paths.
        changed_csv: String,
    },
    /// Validate that the persisted graph round-trips.
    Validate {
        /// SQLite database path (defaults to ~/.jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Query the code graph and emit a JSON pack.
    Query {
        /// Workspace root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Repository id.
        #[arg(long, default_value = "local")]
        repo_id: String,
        /// Repository owner.
        #[arg(long, default_value = "local")]
        owner: String,
        /// Repository name.
        #[arg(long, default_value = "workspace")]
        name: String,
        /// Git ref name.
        #[arg(long, default_value = "HEAD")]
        ref_name: String,
        /// Commit SHA.
        #[arg(long, default_value = "unknown")]
        commit_sha: String,
    },
}

fn resolve_db(db: Option<PathBuf>) -> PathBuf {
    db.unwrap_or_else(default_db_path)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Index { root, db } => {
            let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
            let graph = CodeGraph::index(&root).context("index workspace")?;
            graph.persist(&store).context("persist graph")?;
            let snapshot = graph.snapshot();
            println!(
                "indexed {} symbols, {} crate-dep edges -> {}",
                snapshot.symbols.len(),
                snapshot.crate_deps.len(),
                store.path().display()
            );
        }
        Commands::Impact { root, db, paths } => {
            let _store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
            let workspace = WorkspaceGraph::load(&root).context("load workspace")?;
            let graph = CodeGraph::index_workspace(&workspace).context("index workspace")?;
            let report = graph
                .try_impact_of(&workspace, &paths)
                .context("impact analysis")?;
            println!("changed_crates:");
            for c in &report.changed_crates {
                println!("  {c}");
            }
            println!("affected_crates:");
            for c in &report.affected_crates {
                println!("  {c}");
            }
            println!("affected_symbols: {}", report.affected_symbols.len());
        }
        Commands::SliceCheck {
            prefixes_csv,
            changed_csv,
        } => {
            let prefixes = split_csv(&prefixes_csv);
            let changed = split_csv(&changed_csv);
            let slice = Slice::new(prefixes);
            match slice.slice_permits(&changed) {
                Ok(()) => println!("slice OK: all {} path(s) in slice", changed.len()),
                Err(out) => {
                    eprintln!("slice DENIED:");
                    for p in &out.out_of_slice_paths {
                        eprintln!("  out-of-slice: {p}");
                    }
                    std::process::exit(1);
                }
            }
        }
        Commands::Validate { db } => {
            let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
            let snapshot = store.load_snapshot().context("load snapshot")?;
            println!(
                "valid: {} symbols, {} crate-dep edges",
                snapshot.symbols.len(),
                snapshot.crate_deps.len()
            );
        }
        Commands::Query {
            root,
            db,
            repo_id,
            owner,
            name,
            ref_name,
            commit_sha,
        } => {
            let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
            let repo = RepoIdentity {
                repo_id,
                owner,
                name,
            };
            let scope = QueryOptions::default();
            let (snapshot, receipt) = store
                .query_or_refresh(&repo, &ref_name, &commit_sha, &root, &scope, || {
                    let workspace = WorkspaceGraph::load(&root)
                        .map_err(|e| jeryu_codegraph::CodeGraphError::Workspace(e.to_string()))?;
                    let graph = CodeGraph::index_workspace(&workspace)
                        .map_err(|e| jeryu_codegraph::CodeGraphError::Storage(e.to_string()))?;
                    Ok(graph.snapshot().clone())
                })
                .context("query code graph")?;
            let pack = QueryPack {
                receipt,
                symbols: snapshot.symbols.len(),
                crate_deps: snapshot.crate_deps.len(),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&pack).context("serialize query pack")?
            );
        }
    }
    Ok(())
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Serialize)]
struct QueryPack {
    receipt: jeryu_codegraph::IndexReceipt,
    symbols: usize,
    crate_deps: usize,
}
