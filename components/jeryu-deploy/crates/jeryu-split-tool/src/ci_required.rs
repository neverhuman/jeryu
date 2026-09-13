//! Validate GitHub's complete job inventory for one source and workflow attempt.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, ensure};
use clap::Args;
use serde::Deserialize;

use crate::audit_score::JsonObject;

const LANES: [&str; 14] = [
    "source",
    "public",
    "rust",
    "web",
    "runtime",
    "product",
    "security",
    "sandbox",
    "oci",
    "splits",
    "legacy",
    "auxiliary",
    "audit",
    "auditor",
];

#[derive(Debug, Args)]
pub(super) struct Arguments {
    #[arg(long)]
    run: PathBuf,
    /// Concatenated pages from the exact workflow-attempt jobs endpoint.
    #[arg(long)]
    jobs: PathBuf,
    #[arg(long)]
    repository: String,
    #[arg(long)]
    source: String,
    #[arg(long)]
    run_id: u64,
    #[arg(long)]
    attempt: u64,
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
}

#[derive(Deserialize)]
struct Run {
    id: u64,
    run_attempt: u64,
    head_sha: String,
    path: String,
    name: String,
    repository: JsonObject<Repository>,
}

#[derive(Deserialize)]
struct Page {
    total_count: usize,
    jobs: Vec<JsonObject<Job>>,
}

#[derive(Deserialize)]
struct Job {
    id: u64,
    run_id: u64,
    run_attempt: u64,
    head_sha: String,
    workflow_name: String,
    name: String,
    status: String,
    conclusion: Option<String>,
}

fn verify(arguments: &Arguments, run: &[u8], pages: &[u8]) -> Result<()> {
    ensure!(
        arguments.run_id > 0 && arguments.attempt > 0,
        "positive run and attempt required"
    );
    ensure!(
        crate::audit_scheduler::hex(&arguments.source, 40),
        "full source SHA required"
    );
    let JsonObject(run): JsonObject<Run> = serde_json::from_slice(run)?;
    ensure!(
        run.id == arguments.run_id
            && run.run_attempt == arguments.attempt
            && run.head_sha == arguments.source
            && run.repository.0.full_name == arguments.repository
            && run.name == "Jeryu"
            && run.path == ".github/workflows/ci.yml",
        "workflow run does not match the expected repository, source, workflow and attempt"
    );
    let expected: BTreeSet<String> = LANES
        .iter()
        .map(|lane| format!("verify / {lane}"))
        .collect();
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut count = 0;
    let mut page_count = 0;
    for page in serde_json::Deserializer::from_slice(pages).into_iter::<JsonObject<Page>>() {
        let page = page?.0;
        page_count += 1;
        ensure!(
            page.total_count == expected.len() + 1,
            "unexpected total job inventory"
        );
        ensure!(!page.jobs.is_empty(), "empty jobs page");
        for JsonObject(job) in page.jobs {
            count += 1;
            ensure!(
                job.id > 0 && ids.insert(job.id),
                "duplicate or invalid job ID"
            );
            ensure!(
                names.insert(job.name.clone()),
                "duplicate job name: {}",
                job.name
            );
            ensure!(
                job.run_id == arguments.run_id
                    && job.run_attempt == arguments.attempt
                    && job.head_sha == arguments.source
                    && job.workflow_name == "Jeryu",
                "job {} belongs to another source or workflow attempt",
                job.name
            );
            if job.name == "jeryu/required" {
                ensure!(
                    job.status == "in_progress" && job.conclusion.is_none(),
                    "aggregate must validate its own running attempt"
                );
            } else {
                ensure!(expected.contains(&job.name), "unexpected job: {}", job.name);
                ensure!(
                    job.status == "completed" && job.conclusion.as_deref() == Some("success"),
                    "required job {} did not complete successfully",
                    job.name
                );
            }
        }
    }
    ensure!(
        page_count > 0 && count == expected.len() + 1,
        "incomplete job pagination"
    );
    ensure!(
        names.remove("jeryu/required") && names == expected,
        "missing required jobs"
    );
    Ok(())
}

pub(super) fn run(arguments: Arguments) -> Result<()> {
    verify(
        &arguments,
        &fs::read(&arguments.run)?,
        &fs::read(&arguments.jobs)?,
    )?;
    println!(
        "All {} required jobs passed for {} at run {} attempt {}",
        LANES.len(),
        arguments.source,
        arguments.run_id,
        arguments.attempt
    );
    Ok(())
}

#[cfg(test)]
#[path = "ci_required_tests.rs"]
mod tests;
