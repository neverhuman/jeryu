use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::{BenchVariantResult, ScenarioReport};

/// Typed extraction of witness note fields, with explicit defaults kept in one place.
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

#[derive(Debug)]
pub(crate) struct RepoShapeDelta {
    pub(crate) monolith_context_bytes: u64,
    pub(crate) arcified_context_bytes: u64,
    pub(crate) context_reduction_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct WitnessScenarioSpec {
    id: String,
    expected_classification: String,
    expect_escalation: bool,
}

pub(crate) fn witness_metrics(root: &Path, witness_loop: &ScenarioReport) -> Result<(f64, usize)> {
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

pub(crate) fn repo_shape_delta(report: &ScenarioReport) -> RepoShapeDelta {
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

pub(crate) fn build_variant_result(
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

pub(crate) fn repair_packet_enrichment_success(root: &Path) -> Result<bool> {
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
