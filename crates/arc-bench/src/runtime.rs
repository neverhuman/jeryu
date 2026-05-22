#![allow(clippy::ptr_arg)]
use std::process::Command;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::model::{BenchVariantResult, ScenarioReport};

#[path = "runtime_variants.rs"]
mod runtime_variants;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RuntimeVariant {
    Baseline,
    ActorAsync,
}

impl RuntimeVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeVariant::Baseline => "baseline-locks",
            RuntimeVariant::ActorAsync => "actor-async",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "baseline-locks" => Ok(Self::Baseline),
            "actor-async" => Ok(Self::ActorAsync),
            _ => anyhow::bail!("unknown runtime variant: {value}"),
        }
    }
}

pub fn run(output: &std::path::Path) -> Result<ScenarioReport> {
    let exe = std::env::current_exe().context("failed to locate arc-bench executable")?;
    let variants = [RuntimeVariant::Baseline, RuntimeVariant::ActorAsync];
    let mut results = Vec::new();
    for variant in variants {
        let output = Command::new(&exe)
            .arg("internal-runtime")
            .arg("--variant")
            .arg(variant.as_str())
            .arg("--ops")
            .arg("12000")
            .arg("--workers")
            .arg("4")
            .arg("--key-space")
            .arg("1024")
            .output()
            .with_context(|| format!("failed to run {}", variant.as_str()))?;
        if !output.status.success() {
            anyhow::bail!(
                "runtime child failed for {}: {}",
                variant.as_str(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let result: BenchVariantResult = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("failed to parse runtime output for {}", variant.as_str()))?;
        results.push(result);
    }

    let report = ScenarioReport {
        scenario: "runtime".to_string(),
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        results,
        cases: Vec::new(),
        notes: vec![
            "The baseline uses a shared Mutex<HashMap<..>> across worker threads.".to_string(),
            "The actor-async variant uses a current-thread Tokio runtime with an actor-owned state map.".to_string(),
            "This benchmark reports throughput and memory tradeoffs for the current workload.".to_string(),
        ],
    };
    std::fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(report)
}

pub fn run_internal(
    variant: RuntimeVariant,
    ops: usize,
    workers: usize,
    key_space: u64,
) -> Result<BenchVariantResult> {
    match variant {
        RuntimeVariant::Baseline => runtime_variants::baseline_runtime(ops, workers, key_space),
        RuntimeVariant::ActorAsync => runtime_variants::actor_runtime(ops, workers, key_space),
    }
}

fn build_result(
    variant: RuntimeVariant,
    wall: std::time::Duration,
    thread_count_max: u64,
    throughput: f64,
    mut latencies: Vec<f64>,
    notes: Vec<String>,
) -> BenchVariantResult {
    BenchVariantResult {
        scenario: "runtime".to_string(),
        variant: variant.as_str().to_string(),
        wall_time_ms: wall.as_millis() as u64,
        peak_rss_kb: Some(peak_rss_kb()),
        thread_count_max: Some(thread_count_max),
        throughput: Some(throughput),
        latency_p50_ms: percentile(&mut latencies.clone(), 0.50),
        latency_p95_ms: percentile(&mut latencies, 0.95),
        context_files: None,
        context_bytes: None,
        selected_tests: None,
        selected_arcs: None,
        notes,
    }
}

fn percentile(values: &mut Vec<f64>, quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((values.len() as f64 - 1.0) * quantile).round() as usize;
    values.get(index).copied()
}

fn peak_rss_kb() -> u64 {
    let mut usage = zero_rusage();
    // SAFETY: `usage` points to writable memory for `rusage`, and `getrusage`
    // initializes it before we inspect the value when the return status is zero.
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if status != 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        (usage.ru_maxrss as u64) / 1024
    }
    #[cfg(not(target_os = "macos"))]
    {
        usage.ru_maxrss as u64
    }
}

fn zero_rusage() -> libc::rusage {
    libc::rusage {
        ru_utime: Default::default(),
        ru_stime: Default::default(),
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    }
}
