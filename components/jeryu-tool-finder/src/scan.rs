//! `scan`: drive the jeryu-codegraph engine as a library and write the ranked
//! cross-repo clusters to `dossiers/clusters.json` for `dossier` to enrich.
//!
//! Two scopes:
//! * default — the family listed in `--manifest` (parity with the predecessor
//!   scan command),
//! * `--system` — EVERY split family on the host (manifest-sibling discovery,
//!   gitignore-aware walking, overlap merging, categories, pattern families).

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use jeryu_codegraph::{
    CodeGraphStore, ToolBuildScanConfig, ToolBuildScanOptions, ToolBuildScanReport,
    discover_system_repo_roots, scan_tool_build_family, scan_tool_build_system,
};
use serde::Deserialize;

use crate::paths;

#[derive(Args)]
pub struct ScanArgs {
    /// Family manifest listing `[[repo]]` roots (family scope).
    #[arg(long, default_value_os_t = paths::default_manifest())]
    manifest: PathBuf,
    /// Scan EVERY split family on the host instead of one manifest.
    #[arg(long)]
    system: bool,
    /// Codegraph SQLite database (honors JERYU_CODEGRAPH_DB).
    #[arg(long, default_value_os_t = paths::default_db())]
    db: PathBuf,
    /// Synthetic repo id for the persisted cluster rows.
    #[arg(long)]
    repo_id: Option<String>,
    /// Maximum clusters to keep.
    #[arg(long, default_value_t = 200)]
    top: usize,
    /// Minimum distinct repos a cluster must span.
    #[arg(long, default_value_t = 2)]
    min_repos: usize,
    /// Normalized lines per fingerprinted window.
    #[arg(long, default_value_t = 8)]
    window_lines: usize,
    /// Minimum occurrences before a window becomes a cluster.
    #[arg(long, default_value_t = 2)]
    min_occurrences: usize,
    /// Where to write the scan report JSON.
    #[arg(long, default_value_os_t = paths::default_clusters_out())]
    out: PathBuf,
    /// Print live progress to stderr (system scope).
    #[arg(long)]
    progress: bool,
}

pub fn run(args: ScanArgs) -> Result<()> {
    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent).context("create dossiers dir")?;
    }
    let store = CodeGraphStore::open(&args.db).context("open codegraph store")?;

    let report = if args.system {
        scan_system(&args)?
    } else {
        scan_family(&args)?
    };

    store
        .persist_tool_build_report(&report)
        .context("persist scan report")?;
    let inherited = store
        .propagate_ignores_to_merged(&report.clusters)
        .context("propagate ignore feedback onto merged clusters")?;

    let json = serde_json::to_string_pretty(&report).context("encode scan report")?;
    std::fs::write(&args.out, json).with_context(|| format!("write {}", args.out.display()))?;

    eprintln!(
        "[scan] {} files ({} skipped) -> {} cross-repo cluster(s), {} families \
         (min_repos={}, inherited-ignores={}) -> {}",
        report.scanned_files,
        report.skipped_files,
        report.clusters.len(),
        report.families.len(),
        args.min_repos,
        inherited,
        args.out.display()
    );
    for cluster in report.clusters.iter().take(10) {
        eprintln!(
            "  {} {} [{}] repos={} occ={} score={}",
            cluster.cluster_id,
            cluster.language,
            cluster.category.as_str(),
            cluster.repo_count,
            cluster.occurrence_count,
            cluster.score
        );
    }
    Ok(())
}

fn scan_system(args: &ScanArgs) -> Result<ToolBuildScanReport> {
    // Family-discovery parents: the manifest's grandparent (the directory the
    // `*-split` checkouts live in).
    let parents: Vec<PathBuf> = args
        .manifest
        .parent()
        .and_then(|split_root| split_root.parent())
        .map(|parent| vec![parent.to_path_buf()])
        .unwrap_or_default();
    let roots = discover_system_repo_roots(&parents).context("discover split families")?;
    if roots.is_empty() {
        return Err(crate::errors::FinderError::new(
            "discover repos to scan",
            "no repo roots resolved on disk",
            &[
                "run from the finder repo root inside the split checkout",
                "pass --manifest pointing at a family repos.manifest.toml",
            ],
            "fix the manifest path, then rerun `jeryu-tool-finder scan`",
        )
        .into());
    }
    let mut options = ToolBuildScanOptions::system_default();
    options.base.window_lines = args.window_lines;
    options.base.min_occurrences = args.min_occurrences;
    options.base.min_repo_count = args.min_repos;
    options.base.max_clusters = args.top;
    let show_progress = args.progress;
    let repo_id = args
        .repo_id
        .clone()
        .unwrap_or_else(|| "system/host".to_string());
    scan_tool_build_system(&roots, repo_id, "working-tree", &options, &move |event| {
        if show_progress {
            eprintln!(
                "[{}] {}/{} repos {} files={} clusters={}",
                event.phase.as_str(),
                event.repos_done,
                event.repo_total,
                event.current_repo,
                event.files_scanned,
                event.clusters_so_far,
            );
        }
    })
    .context("system scan")
}

fn scan_family(args: &ScanArgs) -> Result<ToolBuildScanReport> {
    #[derive(Deserialize)]
    struct Manifest {
        #[serde(default)]
        repo: Vec<ManifestRepo>,
    }
    #[derive(Deserialize)]
    struct ManifestRepo {
        path: Option<String>,
        name: Option<String>,
    }

    let text = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("read {}", args.manifest.display()))?;
    let manifest: Manifest = toml::from_str(&text)
        .with_context(|| format!("parse {} as a split manifest", args.manifest.display()))?;
    let mut roots = Vec::new();
    for repo in manifest.repo {
        let Some(path) = repo.path.map(PathBuf::from).filter(|path| path.is_dir()) else {
            continue;
        };
        let repo_id = repo.name.unwrap_or_else(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("repo")
                .to_string()
        });
        roots.push((repo_id, path));
    }
    if roots.is_empty() {
        return Err(crate::errors::FinderError::new(
            "discover repos to scan",
            "no repo roots resolved on disk",
            &[
                "run from the finder repo root inside the split checkout",
                "pass --manifest pointing at a family repos.manifest.toml",
            ],
            "fix the manifest path, then rerun `jeryu-tool-finder scan`",
        )
        .into());
    }
    let repo_id = args
        .repo_id
        .clone()
        .unwrap_or_else(|| "family/jeryu-split".to_string());
    scan_tool_build_family(
        &roots,
        repo_id,
        "working-tree",
        ToolBuildScanConfig {
            window_lines: args.window_lines,
            min_occurrences: args.min_occurrences,
            min_repo_count: args.min_repos,
            max_clusters: args.top,
            ..ToolBuildScanConfig::default()
        },
    )
    .context("family scan")
}
