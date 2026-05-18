use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use cargo_aer::scan_workspace;
use chrono::Utc;

use crate::model::{BenchVariantResult, ExceptionCaseResult, ExceptionCaseSpec, ScenarioReport};

pub fn run(output: &Path) -> Result<ScenarioReport> {
    let root = workspace_root();
    let manifest_path = root.join("proof/examples/labs/exception-zoo/manifest.json");
    let payload = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let specs: Vec<ExceptionCaseSpec> = serde_json::from_str(&payload)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let mut results = Vec::new();
    let mut cases = Vec::new();
    for spec in specs {
        if !spec.benchmarkable {
            continue;
        }
        let start = Instant::now();
        let (success, observed_signal) = match spec.mode.as_str() {
            "scan" => run_scan_case(&root, &spec)?,
            "test" => run_cargo_case(&root, &spec, "test")?,
            _ => run_cargo_case(&root, &spec, "check")?,
        };
        let wall_time_ms = start.elapsed().as_millis() as u64;
        cases.push(ExceptionCaseResult {
            case_id: spec.case_id.clone(),
            category: spec.category.clone(),
            failure_mode: spec.failure_mode.clone(),
            expected_signal: spec.expected_signal.clone(),
            observed_signal: observed_signal.clone(),
            success,
            wall_time_ms,
            docs: spec.docs.clone(),
        });
        results.push(BenchVariantResult {
            scenario: "exceptions".to_string(),
            variant: spec.case_id.clone(),
            wall_time_ms,
            peak_rss_kb: None,
            thread_count_max: None,
            throughput: None,
            latency_p50_ms: None,
            latency_p95_ms: None,
            context_files: None,
            context_bytes: None,
            selected_tests: None,
            selected_arcs: None,
            notes: vec![
                format!("mode: {}", spec.mode),
                format!("success: {}", success),
                format!("observed: {}", observed_signal),
            ],
        });
    }

    let report = ScenarioReport {
        scenario: "exceptions".to_string(),
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        results,
        cases,
        notes: vec![
            "The exception zoo mixes compile failures, runtime failures, and structural scan findings.".to_string(),
            "These fixtures model common vibe-coding failure modes that agents should normalize instead of hiding.".to_string(),
        ],
    };
    fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(report)
}

fn run_scan_case(root: &Path, spec: &ExceptionCaseSpec) -> Result<(bool, String)> {
    let manifest = root.join(&spec.manifest_path);
    let report = scan_workspace(Some(&manifest))?;
    let observed = match report
        .findings
        .iter()
        .find(|finding| finding.class_id == spec.expected_signal)
    {
        Some(finding) => finding.class_id.clone(),
        None => match report.findings.first() {
            Some(finding) => finding.class_id.clone(),
            None => "no-finding".to_string(),
        },
    };
    Ok((observed == spec.expected_signal, observed))
}

fn run_cargo_case(
    root: &Path,
    spec: &ExceptionCaseSpec,
    subcommand: &str,
) -> Result<(bool, String)> {
    let manifest = root.join(&spec.manifest_path);
    let mut command = Command::new("cargo");
    command
        .arg(subcommand)
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest);
    command.args(&spec.cargo_args);
    let output = command
        .output()
        .with_context(|| format!("failed to run cargo {subcommand} for {}", spec.case_id))?;
    let combined = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);
    let observed = first_signal(&combined);
    let success = combined.contains(&spec.expected_signal);
    Ok((success, observed))
}

fn first_signal(output: &str) -> String {
    if let Some(line) = output
        .lines()
        .find(|line| line.contains("error") || line.contains("panicked"))
    {
        line.trim().to_string()
    } else {
        "no-error-line-found".to_string()
    }
}

fn workspace_root() -> PathBuf {
    crate::support::workspace_root()
}
