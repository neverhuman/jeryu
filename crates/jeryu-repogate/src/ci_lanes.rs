//! CI lane manifest and hosted-workflow drift guard.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::GateOutcome;

mod workflow_runs;
use workflow_runs::{
    extract_run_commands, normalize_command, workflow_declares_job, workflow_files,
};

/// Committed CI lane manifest path.
pub const CI_LANES_RELATIVE_PATH: &str = "agent/ci-lanes.toml";

#[derive(Debug, Clone, Deserialize)]
struct CiLaneManifest {
    schema_version: u32,
    worker_count: u32,
    jankurai_version: String,
    #[serde(default)]
    allowed_setup_commands: Vec<String>,
    #[serde(default)]
    lanes: Vec<CiLane>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CiLane {
    pub id: String,
    pub workflow: String,
    pub job: String,
    pub command: String,
    #[serde(default)]
    pub full: bool,
}

pub fn run_ci_lanes_check(root: &Path) -> Result<GateOutcome> {
    let manifest = load_manifest(root)?;
    let errors = validate_manifest(root, &manifest)?;
    if errors.is_empty() {
        Ok(GateOutcome {
            stdout: vec![format!(
                "ci-lanes check ok: {} lane(s), {} worker(s), {}",
                manifest.lanes.len(),
                manifest.worker_count,
                manifest.jankurai_version
            )],
            exit_code: 0,
        })
    } else {
        Ok(GateOutcome {
            stdout: errors
                .into_iter()
                .map(|error| format!("ci-lanes drift {error}"))
                .collect(),
            exit_code: 1,
        })
    }
}

pub fn run_ci_lanes_list(root: &Path, full: bool, json: bool) -> Result<GateOutcome> {
    let manifest = load_manifest(root)?;
    let lanes: Vec<CiLane> = manifest
        .lanes
        .into_iter()
        .filter(|lane| !full || lane.full)
        .collect();
    let stdout = if json {
        vec![serde_json::to_string_pretty(&lanes)?]
    } else {
        lanes
            .into_iter()
            .map(|lane| format!("{}\t{}", lane.id, lane.command))
            .collect()
    };
    Ok(GateOutcome {
        stdout,
        exit_code: 0,
    })
}

fn load_manifest(root: &Path) -> Result<CiLaneManifest> {
    let path = root.join(CI_LANES_RELATIVE_PATH);
    let raw =
        fs::read_to_string(&path).with_context(|| format!("read {}", CI_LANES_RELATIVE_PATH))?;
    toml::from_str(&raw).with_context(|| format!("parse {}", CI_LANES_RELATIVE_PATH))
}

fn validate_manifest(root: &Path, manifest: &CiLaneManifest) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    if manifest.schema_version != 1 {
        errors.push(format!(
            "unsupported schema_version {} in {}",
            manifest.schema_version, CI_LANES_RELATIVE_PATH
        ));
    }
    if manifest.worker_count != 40 {
        errors.push(format!(
            "worker_count must stay 40 for local/hosted parity, got {}",
            manifest.worker_count
        ));
    }
    if manifest.jankurai_version.trim().is_empty() {
        errors.push("jankurai_version must be set".to_string());
    }
    if manifest.lanes.is_empty() {
        errors.push("manifest has no lanes".to_string());
    }

    let mut ids = BTreeSet::new();
    let mut lanes_by_workflow: BTreeMap<String, Vec<&CiLane>> = BTreeMap::new();
    for lane in &manifest.lanes {
        if lane.id.trim().is_empty() {
            errors.push("lane with empty id".to_string());
        }
        if !ids.insert(lane.id.clone()) {
            errors.push(format!("duplicate lane id {}", lane.id));
        }
        if lane.workflow.trim().is_empty() {
            errors.push(format!("lane {} has empty workflow", lane.id));
        }
        if lane.job.trim().is_empty() {
            errors.push(format!("lane {} has empty job", lane.id));
        }
        if lane.command.trim().is_empty() {
            errors.push(format!("lane {} has empty command", lane.id));
        }
        if !root.join(&lane.workflow).exists() {
            errors.push(format!(
                "lane {} references missing workflow {}",
                lane.id, lane.workflow
            ));
        }
        lanes_by_workflow
            .entry(lane.workflow.clone())
            .or_default()
            .push(lane);
    }

    let declared_workflows: BTreeSet<_> = manifest
        .lanes
        .iter()
        .map(|lane| lane.workflow.clone())
        .collect();
    for workflow in workflow_files(root)? {
        if !declared_workflows.contains(&workflow) {
            errors.push(format!("workflow {workflow} is not declared in manifest"));
        }
    }

    let setup_commands: BTreeSet<_> = manifest
        .allowed_setup_commands
        .iter()
        .map(|command| normalize_command(command))
        .collect();
    for (workflow, lanes) in lanes_by_workflow {
        let workflow_path = root.join(&workflow);
        if !workflow_path.exists() {
            continue;
        }
        let text = fs::read_to_string(&workflow_path)
            .with_context(|| format!("read workflow {}", workflow))?;
        let run_commands: Vec<String> = extract_run_commands(&text)
            .into_iter()
            .map(|command| normalize_command(&command))
            .collect();
        let run_set: BTreeSet<_> = run_commands.iter().cloned().collect();

        for lane in &lanes {
            if !workflow_declares_job(&text, &lane.job) {
                errors.push(format!(
                    "workflow {} does not declare manifest job {} for lane {}",
                    workflow, lane.job, lane.id
                ));
            }
            let expected = normalize_command(&lane.command);
            if !run_set.contains(&expected) {
                errors.push(format!(
                    "workflow {} is missing lane {} command `{}`",
                    workflow, lane.id, lane.command
                ));
            }
        }

        let lane_commands: BTreeSet<_> = lanes
            .iter()
            .map(|lane| normalize_command(&lane.command))
            .collect();
        for command in &run_commands {
            if setup_commands.contains(command) || lane_commands.contains(command) {
                continue;
            }
            errors.push(format!(
                "workflow {} has undeclared run command `{}`",
                workflow, command
            ));
        }
    }

    Ok(errors)
}
