//! `jeryu-codegraph` CLI: index the workspace, query impact, and check slices.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jeryu_codegraph::{
    CodeGraph, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore, Slice,
    ToolBuildScanConfig, default_db_path, scan_tool_build_clusters,
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
        /// SQLite database path (defaults to ~/.jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Fast repeated-code cluster mining for Jankurai tool-building leads.
    ToolBuild {
        #[command(subcommand)]
        command: ToolBuildCommands,
    },
}

#[derive(Subcommand)]
enum ToolBuildCommands {
    /// Scan a root and persist ranked repeated-code clusters.
    Scan {
        /// Repository root to scan.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Stable repo id for persisted cluster rows.
        #[arg(long)]
        repo_id: Option<String>,
        /// Commit/ref label for persisted cluster rows.
        #[arg(long)]
        commit: Option<String>,
        /// Number of normalized non-empty lines per fingerprinted window.
        #[arg(long, default_value_t = 8)]
        window_lines: usize,
        /// Minimum occurrences before a fingerprint becomes a cluster.
        #[arg(long, default_value_t = 2)]
        min_occurrences: usize,
        /// Maximum clusters to persist/return.
        #[arg(long, default_value_t = 50)]
        top: usize,
        /// Emit full JSON report.
        #[arg(long)]
        json: bool,
    },
    /// List persisted tool-building clusters.
    Clusters {
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Optional repo id filter.
        #[arg(long)]
        repo_id: Option<String>,
        /// Maximum clusters to return.
        #[arg(long, default_value_t = 50)]
        top: usize,
        /// Include ignored clusters in output.
        #[arg(long)]
        include_ignored: bool,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Ignore/suppress a cluster with an auditable reason.
    Ignore {
        /// SQLite database path (defaults to ~/.local/share/jeryu/codegraph.sqlite).
        #[arg(long)]
        db: Option<PathBuf>,
        /// Cluster id to ignore.
        cluster_id: String,
        /// Required ignore reason.
        #[arg(long)]
        reason: String,
        /// Actor recording feedback.
        #[arg(long, default_value = "operator")]
        ignored_by: String,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
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
        Commands::ToolBuild { command } => match command {
            ToolBuildCommands::Scan {
                root,
                db,
                repo_id,
                commit,
                window_lines,
                min_occurrences,
                top,
                json,
            } => {
                let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
                let repo_id = repo_id.unwrap_or_else(|| {
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("local")
                        .to_string()
                });
                let commit = commit
                    .or_else(|| git_head(&root))
                    .unwrap_or_else(|| "working-tree".to_string());
                let report = scan_tool_build_clusters(
                    &root,
                    repo_id,
                    commit,
                    ToolBuildScanConfig {
                        window_lines,
                        min_occurrences,
                        max_clusters: top,
                        ..ToolBuildScanConfig::default()
                    },
                )
                .context("scan tool-build clusters")?;
                store
                    .persist_tool_build_report(&report)
                    .context("persist tool-build clusters")?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "tool-build scan: files={} skipped={} clusters={} -> {}",
                        report.scanned_files,
                        report.skipped_files,
                        report.clusters.len(),
                        store.path().display()
                    );
                    for cluster in &report.clusters {
                        println!(
                            "  {} score={} occurrences={} files={} {}",
                            cluster.cluster_id,
                            cluster.score,
                            cluster.occurrence_count,
                            cluster.file_count,
                            cluster.insight
                        );
                    }
                }
            }
            ToolBuildCommands::Clusters {
                db,
                repo_id,
                top,
                include_ignored,
                json,
            } => {
                let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
                let clusters = store
                    .tool_build_clusters(repo_id.as_deref(), top, include_ignored)
                    .context("load tool-build clusters")?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&clusters)?);
                } else {
                    println!("tool-build clusters: {}", clusters.len());
                    for cluster in &clusters {
                        let ignored = cluster
                            .ignored
                            .as_ref()
                            .map(|i| format!(" ignored={}", i.reason))
                            .unwrap_or_default();
                        println!(
                            "  {} score={} occurrences={} files={}{}",
                            cluster.cluster_id,
                            cluster.score,
                            cluster.occurrence_count,
                            cluster.file_count,
                            ignored
                        );
                    }
                }
            }
            ToolBuildCommands::Ignore {
                db,
                cluster_id,
                reason,
                ignored_by,
                json,
            } => {
                let store = CodeGraphStore::open(resolve_db(db)).context("open store")?;
                let ignored = store
                    .ignore_tool_build_cluster(&cluster_id, &reason, &ignored_by)
                    .context("ignore tool-build cluster")?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&ignored)?);
                } else {
                    println!(
                        "ignored {} by {}: {}",
                        ignored.cluster_id, ignored.ignored_by, ignored.reason
                    );
                }
            }
        },
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
