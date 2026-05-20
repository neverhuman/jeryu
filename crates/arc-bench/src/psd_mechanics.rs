use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::model::{BenchVariantResult, ScenarioReport};

/// Typed extraction of witness note fields — one parse call per result,
/// with explicit defaults documented here rather than at each call site.
struct WitnessNotes {
    /// Empty string when the "Classification" key is absent from notes.
    classification: String,
    /// False when the "Escalated" key is absent or not "true".
    escalated: bool,
}

impl WitnessNotes {
    fn parse(notes: &[String]) -> Self {
        let classification = match extract_note_value(notes, "Classification") {
            Some(v) => v.to_owned(),
            None => String::new(),
        };
        let escalated = match extract_note_value(notes, "Escalated") {
            Some(v) => v == "true",
            None => false,
        };
        Self {
            classification,
            escalated,
        }
    }
}

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
                format!("False-positive boundary escalations eliminated: {}.", false_positive_eliminations),
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
                format!("Exception-zoo detections: {exception_successes}/{}.", exceptions.cases.len()),
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

#[derive(Debug)]
struct RepoShapeDelta {
    monolith_context_bytes: u64,
    arcified_context_bytes: u64,
    context_reduction_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct WitnessScenarioSpec {
    id: String,
    expected_classification: String,
    expect_escalation: bool,
}

fn witness_metrics(root: &Path, witness_loop: &ScenarioReport) -> Result<(f64, usize)> {
    let manifest = root.join("proof/labs/witness-bench/manifest.json");
    let scenarios: Vec<WitnessScenarioSpec> = serde_json::from_str(
        &fs::read_to_string(&manifest)
            .with_context(|| format!("failed to read {}", manifest.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest.display()))?;

    let mut correct = 0usize;
    let mut total = 0usize;
    let mut false_positive_eliminations = 0usize;

    for scenario in scenarios {
        let baseline_variant = format!("{}-baseline", scenario.id);
        let witnessed_variant = format!("{}-witnessed", scenario.id);
        let baseline = witness_loop
            .results
            .iter()
            .find(|result| result.variant == baseline_variant)
            .context("missing baseline witness-loop result")?;
        let witnessed = witness_loop
            .results
            .iter()
            .find(|result| result.variant == witnessed_variant)
            .context("missing witnessed witness-loop result")?;

        let witnessed_notes = WitnessNotes::parse(&witnessed.notes);
        let baseline_notes = WitnessNotes::parse(&baseline.notes);

        if witnessed_notes.classification == scenario.expected_classification
            && witnessed_notes.escalated == scenario.expect_escalation
        {
            correct += 1;
        }
        if baseline_notes.escalated && !scenario.expect_escalation && !witnessed_notes.escalated {
            false_positive_eliminations += 1;
        }
        total += 1;
    }

    let accuracy = if total == 0 {
        0.0
    } else {
        correct as f64 / total as f64
    };
    Ok((accuracy, false_positive_eliminations))
}

fn repo_shape_delta(report: &ScenarioReport) -> RepoShapeDelta {
    let monolith = report
        .results
        .iter()
        .find(|result| result.variant == "monolith")
        .and_then(|result| result.context_bytes)
        .unwrap_or(0);
    let arcified = report
        .results
        .iter()
        .find(|result| result.variant == "arcified")
        .and_then(|result| result.context_bytes)
        .unwrap_or(0);
    let ratio = if arcified == 0 {
        0.0
    } else {
        monolith as f64 / arcified as f64
    };
    RepoShapeDelta {
        monolith_context_bytes: monolith,
        arcified_context_bytes: arcified,
        context_reduction_ratio: (ratio * 100.0).round() / 100.0,
    }
}

fn build_variant_result(
    variant: &str,
    wall_time_ms: u64,
    context_files: Option<usize>,
    context_bytes: Option<u64>,
    selected_tests: Option<usize>,
    selected_arcs: Option<usize>,
    notes: Vec<String>,
) -> BenchVariantResult {
    BenchVariantResult {
        scenario: "psd-mechanics".to_string(),
        variant: variant.to_string(),
        wall_time_ms,
        peak_rss_kb: None,
        thread_count_max: None,
        throughput: None,
        latency_p50_ms: None,
        latency_p95_ms: None,
        context_files,
        context_bytes,
        selected_tests,
        selected_arcs,
        notes,
    }
}

fn repair_packet_enrichment_success(root: &Path) -> Result<bool> {
    let output_dir = root.join("target/agent");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let packet = witness_rt::RepairPacket {
        code: "PSD-DEMO".to_string(),
        message: "demo runtime failure".to_string(),
        file: "src/lib.rs".to_string(),
        line: 9,
        column: 3,
        cell: Some("pricing-engine".to_string()),
        cell_purpose: Some("Synthetic repair-bundle enrichment check".to_string()),
        match_provenance: Some("synthetic-longest-owned-path-prefix".to_string()),
        matched_owned_path: Some("src/".to_string()),
        invariants: vec!["pricing stays pure".to_string()],
        likely_causes: vec!["demo failure".to_string()],
        hints: vec!["inspect local pricing logic first".to_string()],
        local_commands: vec!["cargo test -p pricing-engine".to_string()],
        escalate_commands: vec![],
        timestamp: witness_rt::current_timestamp(),
    };
    fs::write(
        output_dir.join("last-failure.json"),
        serde_json::to_string_pretty(&packet)?,
    )
    .with_context(|| format!("failed to write {}", output_dir.display()))?;

    let bundle = cargo_witness::repair::build_repair_bundle(root)?;
    Ok(bundle.status == "action-required"
        && bundle
            .notes
            .iter()
            .any(|note| note.contains("synthetic-longest-owned-path-prefix"))
        && bundle
            .validate_after_fix
            .iter()
            .any(|command| command.contains("pricing-engine")))
}

fn extract_note_value<'a>(notes: &'a [String], prefix: &str) -> Option<&'a str> {
    notes.iter().find_map(|note| {
        note.strip_prefix(prefix)
            .and_then(|value| value.strip_prefix(": "))
    })
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
