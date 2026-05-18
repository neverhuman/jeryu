#![allow(clippy::single_char_add_str)]
mod exceptions;
mod model;
mod psd_mechanics;
mod repo_shape;
mod runtime;
mod support;
mod witness_loop;

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};

use crate::model::ScenarioReport;
use crate::runtime::RuntimeVariant;

#[derive(Parser)]
#[command(name = "arc-bench")]
#[command(about = "Benchmarked demos for the Agent-Native Rust Standard")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(value_enum)]
        scenario: Scenario,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Report {
        #[arg(long, default_value = "bench-results")]
        input_dir: PathBuf,
        #[arg(long, default_value = "bench-results/comparison.md")]
        output: PathBuf,
    },
    #[command(hide = true)]
    InternalRuntime {
        #[arg(long)]
        variant: String,
        #[arg(long)]
        ops: usize,
        #[arg(long)]
        workers: usize,
        #[arg(long)]
        key_space: u64,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Scenario {
    #[value(name = "repo-shape")]
    RepoShape,
    Runtime,
    Exceptions,
    #[value(name = "witness-loop")]
    WitnessLoop,
    #[value(name = "psd-mechanics")]
    PsdMechanics,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("arc-bench error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run { scenario, output } => {
            let output = match output {
                Some(path) => path,
                None => default_output(scenario),
            };
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            let report = match scenario {
                Scenario::RepoShape => repo_shape::run(&output)?,
                Scenario::Runtime => runtime::run(&output)?,
                Scenario::Exceptions => exceptions::run(&output)?,
                Scenario::WitnessLoop => witness_loop::run(&output)?,
                Scenario::PsdMechanics => psd_mechanics::run(&output)?,
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Report { input_dir, output } => {
            let mut reports = Vec::new();
            for entry in fs::read_dir(&input_dir)
                .with_context(|| format!("failed to read {}", input_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let payload = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let report: ScenarioReport = serde_json::from_str(&payload)
                    .with_context(|| format!("failed to parse {}", path.display()))?;
                reports.push(report);
            }
            reports.sort_by(|left, right| left.scenario.cmp(&right.scenario));
            let markdown = build_report(&reports);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&output, markdown)
                .with_context(|| format!("failed to write {}", output.display()))?;
            println!("{}", output.display());
        }
        Command::InternalRuntime {
            variant,
            ops,
            workers,
            key_space,
        } => {
            let variant = RuntimeVariant::parse(&variant)?;
            let result = runtime::run_internal(variant, ops, workers, key_space)?;
            println!("{}", serde_json::to_string(&result)?);
        }
    }
    Ok(())
}

fn default_output(scenario: Scenario) -> PathBuf {
    let name = match scenario {
        Scenario::RepoShape => "repo-shape",
        Scenario::Runtime => "runtime",
        Scenario::Exceptions => "exceptions",
        Scenario::WitnessLoop => "witness-loop",
        Scenario::PsdMechanics => "psd-mechanics",
    };
    PathBuf::from(format!("bench-results/{name}.json"))
}

fn build_report(reports: &[ScenarioReport]) -> String {
    let mut output = String::new();
    output.push_str("# Proof-Scoped Benchmark Comparison\n\n");
    output.push_str(&format!("Generated: {}\n\n", Utc::now().format("%Y-%m-%d")));
    for report in reports {
        output.push_str(&format!("## {}\n\n", report.scenario));
        output.push_str("| Variant | Wall ms | Selected ARCs | Selected tests | Context files | Context bytes | Throughput | P50 ms | P95 ms |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for result in &report.results {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                result.variant,
                result.wall_time_ms,
                render_option(result.selected_arcs),
                render_option(result.selected_tests),
                render_option(result.context_files),
                render_option(result.context_bytes),
                render_float(result.throughput),
                render_float(result.latency_p50_ms),
                render_float(result.latency_p95_ms),
            ));
        }
        if !report.notes.is_empty() {
            output.push_str("\n");
            for note in &report.notes {
                output.push_str(&format!("- {}\n", note));
            }
            output.push('\n');
        }
        if !report.cases.is_empty() {
            output.push_str("| Case | Success | Expected | Observed | Wall ms |\n");
            output.push_str("| --- | --- | --- | --- | --- |\n");
            for case in &report.cases {
                output.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    case.case_id,
                    case.success,
                    case.expected_signal.replace('|', "\\|"),
                    case.observed_signal.replace('|', "\\|"),
                    case.wall_time_ms
                ));
            }
            output.push('\n');
        }
    }
    output
}

fn render_option<T: std::fmt::Display>(value: Option<T>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "-".to_string(),
    }
}

fn render_float(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.2}"),
        None => "-".to_string(),
    }
}
