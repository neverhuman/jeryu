use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;

use crate::model::ScenarioReport;
use crate::psd_mechanics_support::{
    build_variant_result, repair_packet_enrichment_success, repo_shape_delta, witness_metrics,
};
use crate::support::workspace_root;

pub fn run(output: &Path) -> Result<ScenarioReport> {
    let root = workspace_root();
    let scratch = std::env::temp_dir().join(format!(
        "psd-mechanics-{}",
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        }
    ));
    fs::create_dir_all(&scratch)
        .with_context(|| format!("failed to create {}", scratch.display()))?;

    let repo_shape = crate::repo_shape::run(&scratch.join("repo-shape.json"))?;
    let witness_loop = crate::witness_loop::run(&scratch.join("witness-loop.json"))?;
    let exceptions = crate::exceptions::run(&scratch.join("exceptions.json"))?;

    let (classification_accuracy, false_positive_eliminations) =
        witness_metrics(&root, &witness_loop)?;
    let repo_delta = repo_shape_delta(&repo_shape);
    let exception_successes = exceptions.cases.iter().filter(|case| case.success).count();

    let compile_manifest =
        root.join("proof/examples/labs/exception-zoo/cases/borrow-lifetime/Cargo.toml");
    let compile_root = compile_manifest
        .parent()
        .context("compile fixture manifest has no parent")?;
    let compile_packets =
        cargo_witness::diagnose::diagnose_workspace(compile_root, Some(&compile_manifest))?;
    let compile_routing_success = compile_packets
        .packets
        .iter()
        .find(|packet| packet.level == "error")
        .map(|packet| packet.owning_arc != "<unmatched>" && !packet.local_commands.is_empty())
        .unwrap_or(false);

    let repair_root = scratch.join("repair-runtime");
    let repair_success = repair_packet_enrichment_success(&repair_root)?;

    let results = vec![
        build_variant_result(
            "repo-shape",
            repo_shape.results.iter().map(|result| result.wall_time_ms).sum(),
            repo_shape
                .results
                .iter()
                .find(|result| result.variant == "arcified")
                .and_then(|result| result.context_files),
            repo_shape
                .results
                .iter()
                .find(|result| result.variant == "arcified")
                .and_then(|result| result.context_bytes),
            repo_shape
                .results
                .iter()
                .find(|result| result.variant == "arcified")
                .and_then(|result| result.selected_tests),
            repo_shape
                .results
                .iter()
                .find(|result| result.variant == "arcified")
                .and_then(|result| result.selected_arcs),
            vec![
                format!(
                    "Arcified context bytes: {} vs monolith {} (~{:.2}x smaller).",
                    repo_delta.arcified_context_bytes,
                    repo_delta.monolith_context_bytes,
                    repo_delta.context_reduction_ratio
                ),
                "Measures default context shrinkage for a local business-rule change.".to_string(),
            ],
        ),
        build_variant_result(
            "witness-loop",
            witness_loop.results.iter().map(|result| result.wall_time_ms).sum(),
            None,
            None,
            Some(false_positive_eliminations),
            None,
            vec![
                format!(
                    "Classification accuracy across scripted witness mutations: {:.2}.",
                    classification_accuracy
                ),
                format!(
                    "False-positive boundary escalations eliminated: {}.",
                    false_positive_eliminations
                ),
            ],
        ),
        build_variant_result(
            "exceptions",
            exceptions.results.iter().map(|result| result.wall_time_ms).sum(),
            None,
            None,
            Some(exception_successes),
            None,
            vec![
                format!(
                    "Exception-zoo detections: {exception_successes}/{}.",
                    exceptions.cases.len()
                ),
                "Covers compile failures, structural findings, and runtime-adjacent failures.".to_string(),
            ],
        ),
        build_variant_result(
            "compile-routing",
            0,
            None,
            None,
            Some(compile_packets.summary.total_errors),
            Some(compile_packets.summary.arcs_affected),
            vec![
                format!("Compile diagnostic routing success: {compile_routing_success}."),
                format!(
                    "Borrow-checker fixture produced {} error packet(s) across {} ARC(s).",
                    compile_packets.summary.total_errors,
                    compile_packets.summary.arcs_affected
                ),
            ],
        ),
        build_variant_result(
            "repair-packet",
            0,
            None,
            None,
            None,
            None,
            vec![
                format!("Synthetic runtime repair packet enrichment success: {repair_success}."),
                "Confirms that repair bundles preserve match provenance and suggested local validation.".to_string(),
            ],
        ),
    ];

    let report = ScenarioReport {
        scenario: "psd-mechanics".to_string(),
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        results,
        cases: Vec::new(),
        notes: vec![
            "Aggregates repo-shape, witness-loop, exception, compile-routing, and repair-packet benchmarks.".to_string(),
            "Reports proof routing precision, context shrinkage, and failure enrichment for the current mutation set.".to_string(),
            "Covers the benchmark inputs used by the proof-scoped control plane.".to_string(),
        ],
    };

    fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(report)
}
