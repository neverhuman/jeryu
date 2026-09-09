//! `dossier`: turn raw cross-repo clusters into rich, agent-readable
//! tool-candidate dossiers — everything an agent needs to decide whether a
//! cluster is worth extracting into a shared tool. Enrichment (anticipated
//! LOC saved, suggested kind/name) comes from `jeryu_codegraph::enrich_cluster`,
//! the same functions the live dashboard uses.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use jeryu_codegraph::{ToolBuildCluster, ToolBuildScanReport, enrich_cluster};
use serde::Serialize;

use crate::paths;

#[derive(Args)]
pub struct DossierArgs {
    /// Scan report to enrich (defaults to dossiers/clusters.json).
    #[arg(long, default_value_os_t = paths::default_clusters_out())]
    input: PathBuf,
    /// Output directory for index.json + per-cluster markdown.
    #[arg(long, default_value_os_t = paths::default_dossier_dir())]
    out_dir: PathBuf,
    /// Minimum distinct repos a cluster must span.
    #[arg(long, default_value_t = 2)]
    min_repos: usize,
    /// Run against the bundled fixture and assert the pipeline is healthy.
    #[arg(long)]
    selftest: bool,
}

/// One agent-readable tool-candidate dossier.
#[derive(Debug, Serialize)]
pub struct Dossier {
    pub cluster_id: String,
    pub language: String,
    pub category: String,
    pub repo_count: usize,
    pub occurrence_count: usize,
    pub file_count: usize,
    pub score: u64,
    pub anticipated_loc_saved: usize,
    pub suggested_kind: String,
    pub suggested_name: String,
    pub candidate_repos: Vec<String>,
    pub insight: String,
    pub normalized_preview: String,
    pub examples_by_repo: BTreeMap<String, Vec<String>>,
}

pub fn run(args: DossierArgs) -> Result<()> {
    let source = if args.selftest {
        paths::repo_root()
            .join("fixtures")
            .join("sample-clusters.json")
    } else {
        args.input.clone()
    };
    if !source.is_file() {
        bail!(
            "clusters input not found: {} (run `jeryu-tool-finder scan` first)",
            source.display()
        );
    }
    let text =
        std::fs::read_to_string(&source).with_context(|| format!("read {}", source.display()))?;
    let report: ToolBuildScanReport = serde_json::from_str(&text)
        .with_context(|| format!("parse {} as a scan report", source.display()))?;

    let mut dossiers: Vec<Dossier> = report
        .clusters
        .iter()
        .filter(|cluster| cluster.repo_count >= args.min_repos)
        .map(build_dossier)
        .collect();
    dossiers.sort_by(|a, b| {
        b.anticipated_loc_saved
            .cmp(&a.anticipated_loc_saved)
            .then_with(|| a.cluster_id.cmp(&b.cluster_id))
    });

    if args.selftest {
        assert!(
            !dossiers.is_empty(),
            "selftest: fixture produced no cross-repo dossiers"
        );
        let top = &dossiers[0];
        assert!(
            !top.candidate_repos.is_empty(),
            "selftest: dossier missing candidate repos"
        );
        assert!(
            [
                "rust-crate",
                "ts-lib",
                "react-component",
                "vite-plugin",
                "shell-lib",
                "config-pattern"
            ]
            .contains(&top.suggested_kind.as_str()),
            "selftest: unexpected suggested kind {}",
            top.suggested_kind
        );
        println!(
            "dossier selftest ok: {} dossier(s) from fixture",
            dossiers.len()
        );
        return Ok(());
    }

    std::fs::create_dir_all(&args.out_dir).context("create dossier dir")?;
    let index = serde_json::to_string_pretty(&dossiers).context("encode dossier index")?;
    std::fs::write(args.out_dir.join("index.json"), index).context("write index.json")?;
    for dossier in &dossiers {
        std::fs::write(
            args.out_dir.join(format!("{}.md", dossier.cluster_id)),
            render_markdown(dossier),
        )
        .with_context(|| format!("write {}.md", dossier.cluster_id))?;
    }
    if !report.families.is_empty() {
        let families = serde_json::to_string_pretty(&report.families).context("encode families")?;
        std::fs::write(args.out_dir.join("families.json"), families)
            .context("write families.json")?;
    }

    println!(
        "[dossier] wrote {} dossier(s) -> {}",
        dossiers.len(),
        args.out_dir.display()
    );
    for dossier in dossiers.iter().take(10) {
        println!(
            "  {} {} [{}] repos={} loc_saved~{}",
            dossier.cluster_id,
            dossier.suggested_kind,
            dossier.category,
            dossier.repo_count,
            dossier.anticipated_loc_saved
        );
    }
    Ok(())
}

pub fn build_dossier(cluster: &ToolBuildCluster) -> Dossier {
    let enrichment = enrich_cluster(cluster);
    let mut examples_by_repo: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for occ in &cluster.occurrences {
        let location = format!("{}:{}-{}", occ.path, occ.start_line, occ.end_line);
        let entry = examples_by_repo.entry(occ.repo_id.clone()).or_default();
        if entry.len() < 6 && !entry.contains(&location) {
            entry.push(location);
        }
    }
    Dossier {
        cluster_id: cluster.cluster_id.clone(),
        language: cluster.language.clone(),
        category: cluster.category.as_str().to_string(),
        repo_count: cluster.repo_count,
        occurrence_count: cluster.occurrence_count,
        file_count: cluster.file_count,
        score: cluster.score,
        anticipated_loc_saved: enrichment.anticipated_loc_saved,
        suggested_kind: enrichment.suggested_kind.to_string(),
        suggested_name: enrichment.suggested_name,
        candidate_repos: examples_by_repo.keys().cloned().collect(),
        insight: cluster.insight.clone(),
        normalized_preview: cluster.normalized_preview.clone(),
        examples_by_repo,
    }
}

fn render_markdown(dossier: &Dossier) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Tool candidate: {}\n\n", dossier.cluster_id));
    out.push_str(&format!(
        "- **suggested**: `{}` — {}\n",
        dossier.suggested_kind, dossier.suggested_name
    ));
    out.push_str(&format!("- **category**: `{}`\n", dossier.category));
    out.push_str(&format!(
        "- **spans**: {} repo(s), {} file(s), {} occurrence(s)\n",
        dossier.repo_count, dossier.file_count, dossier.occurrence_count
    ));
    out.push_str(&format!(
        "- **anticipated LOC saved**: {} (score {})\n",
        dossier.anticipated_loc_saved, dossier.score
    ));
    out.push_str(&format!(
        "- **candidate repos**: {}\n\n",
        dossier.candidate_repos.join(", ")
    ));
    out.push_str(&format!("_{}_\n\n## Where it lives\n\n", dossier.insight));
    for (repo, locations) in &dossier.examples_by_repo {
        out.push_str(&format!("### {repo}\n"));
        for location in locations {
            out.push_str(&format!("- `{location}`\n"));
        }
        out.push('\n');
    }
    out.push_str("## Normalized window\n\n```\n");
    out.push_str(&dossier.normalized_preview);
    out.push_str("\n```\n\n## Decision\n\nIf this is worth extracting, run:\n\n```bash\n");
    out.push_str(&format!(
        "jeryu-tool-finder propose {}\n",
        dossier.cluster_id
    ));
    out.push_str(
        "```\n\nwhich files a build task + a `proposed` entry into jeryu-tool's registry.\n",
    );
    out
}
