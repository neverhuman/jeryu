//! `jeryu-codegraph` CLI: index the workspace, query impact, and check slices.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jeryu_codegraph::{
    CodeGraph, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore, Slice,
    default_db_path,
};
use jeryu_rustjet::WorkspaceGraph;

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
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Report impact of changed repo-relative paths.
    Impact {
        /// Workspace root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Changed repo-relative paths.
        paths: Vec<String>,
    },
    /// Build the hosted-oracle impact pack for changed paths.
    Query {
        /// Workspace root.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Changed repo-relative path. Repeat for multiple paths.
        #[arg(long = "changed")]
        changed: Vec<String>,
        /// Optional short task intent.
        #[arg(long)]
        intent: Option<String>,
        /// Optional code question.
        #[arg(long)]
        question: Option<String>,
        /// Requested token budget for downstream context consumers.
        #[arg(long)]
        max_tokens: Option<u32>,
        /// Ref label for the pack.
        #[arg(long = "ref", default_value = "working-tree")]
        ref_name: String,
        /// Emit the full JSON impact pack.
        #[arg(long)]
        json: bool,
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
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
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
            let report = graph.impact_of(&workspace, &paths);
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
        Commands::Query {
            root,
            db,
            changed,
            intent,
            question,
            max_tokens,
            ref_name,
            json,
        } => {
            let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
            let service = CodeGraphService::new(root.clone(), store);
            let repo = CodeGraphRepoIdentity::local(&root);
            let query = CodeGraphQuery {
                ref_name,
                changed_paths: changed,
                intent,
                question,
                max_tokens,
            };
            let commit = git_head(&root).unwrap_or_else(|| "working-tree".to_string());
            let pack = service
                .query(repo, commit, query)
                .context("build codegraph impact pack")?;
            if json {
                println!("{}", serde_json::to_string_pretty(&pack)?);
            } else {
                println!(
                    "{}@{} changed={} affected={} must_read={}",
                    pack.repo.id,
                    pack.commit,
                    pack.changed_crates.len(),
                    pack.affected_crates.len(),
                    pack.must_read_files.len()
                );
                for file in &pack.must_read_files {
                    println!("  must_read: {}", file.path);
                }
            }
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

fn git_head(root: &PathBuf) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
