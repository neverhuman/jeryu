//! Native TOML CI parsing: `[[job]]` tables are lowered into [`PendingJob`]s.

use std::collections::BTreeMap;
use std::str::FromStr;

use jeryu_ci_ir::{
    ArtifactPath, ArtifactWhen, CacheMode, CacheMount, Job, Pipeline, PipelineSource, RunnerClass,
    Step, deterministic_hash, sanitize_id, trim_quotes,
};

use crate::attributes::default_cache_mounts;
use crate::error::{CompileContext, CompileError};
use crate::lexer::{parse_array_or_scalar, strip_comment};
use crate::pipeline::{PendingJob, finish_pipeline};

pub(crate) fn compile_native(
    input: &str,
    context: &CompileContext,
) -> Result<Pipeline, CompileError> {
    let mut pending = Vec::new();
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    let mut current_arrays: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut in_job = false;

    for raw in input.lines() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line == "[[job]]" || line == "[[jobs]]" {
            if in_job {
                pending.push(native_job_from_maps(&current, &current_arrays, context)?);
                current.clear();
                current_arrays.clear();
            }
            in_job = true;
            continue;
        }
        if !in_job {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim();
            if value.starts_with('[') {
                current_arrays.insert(key, parse_array_or_scalar(value));
            } else {
                current.insert(key, trim_quotes(value).to_string());
            }
        } else {
            return Err(CompileError::InvalidLine(line));
        }
    }
    if in_job {
        pending.push(native_job_from_maps(&current, &current_arrays, context)?);
    }
    finish_pipeline(PipelineSource::NativeToml, pending, context)
}

fn native_job_from_maps(
    values: &BTreeMap<String, String>,
    arrays: &BTreeMap<String, Vec<String>>,
    context: &CompileContext,
) -> Result<PendingJob, CompileError> {
    let id = values
        .get("id")
        .cloned()
        .unwrap_or_else(|| "job".to_string());
    let name = values.get("name").cloned().unwrap_or_else(|| id.clone());
    let runner_text = values
        .get("runner_class")
        .or_else(|| values.get("runner"))
        .cloned()
        .unwrap_or_else(|| context.default_runner.as_str().to_string());
    let runner = RunnerClass::from_str(&runner_text)
        .map_err(|_| CompileError::InvalidRunner(runner_text.clone()))?;
    let mut job = Job::new(&id, name, runner);
    let run = arrays
        .get("run")
        .cloned()
        .or_else(|| values.get("run").map(|value| vec![value.clone()]))
        .ok_or_else(|| CompileError::MissingSteps(id.clone()))?;
    if run.iter().all(|command| command.trim().is_empty()) {
        return Err(CompileError::MissingSteps(id));
    }
    for (idx, command) in run.iter().enumerate() {
        job.steps.push(Step::run(
            format!("{id}_run_{idx}"),
            format!("run {idx}"),
            command,
        ));
    }
    if let Some(paths) = arrays.get("artifact_paths") {
        job.artifact_paths.push(ArtifactPath {
            name: format!("{id}-artifacts"),
            paths: paths.clone(),
            when: ArtifactWhen::Always,
            retention_days: 14,
        });
    }
    if let Some(paths) = arrays.get("cache_mounts") {
        job.cache_mounts = paths
            .iter()
            .map(|path| CacheMount {
                name: sanitize_id(path),
                path: path.clone(),
                mode: CacheMode::ReadOnly,
                fingerprint: deterministic_hash(path),
            })
            .collect();
    } else {
        job.cache_mounts = default_cache_mounts(&id);
    }
    if let Some(timeout) = values.get("timeout_seconds") {
        // A malformed `timeout_seconds` defaults to the one-hour job timeout used
        // elsewhere in the IR; the invalid case is the intended fallback rather
        // than a swallowed error.
        job.timeout_seconds = timeout.parse::<u64>().unwrap_or(3600);
    }
    let needs = arrays.get("needs").cloned().unwrap_or_default();
    Ok(PendingJob {
        origin: id,
        job,
        needs: needs.clone(),
    })
}
