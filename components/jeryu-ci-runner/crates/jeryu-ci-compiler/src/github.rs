//! GitHub Actions workflow parsing: walking the `jobs:` map and lowering each
//! job (runner, needs, timeout, permissions, matrix, cache, artifacts) into the
//! intermediate [`PendingJob`] form.

use std::collections::BTreeMap;
use std::str::FromStr;

use jeryu_ci_ir::{Job, Pipeline, PipelineSource, RunnerClass, TokenScope};

use crate::attributes::{
    default_cache_mounts, parse_artifacts, parse_cache_mounts, parse_token_scope,
};
use crate::error::{CompileContext, CompileError};
use crate::lexer::{
    SourceLine, collect_lines, find_line, header_name, is_yaml_map_header, nested_slice,
    parse_array_or_scalar, parse_yaml_list, scalar_after,
};
use crate::matrix::{expanded_id, expanded_name, matrix_combinations, parse_matrix};
use crate::pipeline::{PendingJob, finish_pipeline};
use crate::steps::{parse_steps, steps_with_matrix};

pub(crate) fn compile_github(
    input: &str,
    context: &CompileContext,
) -> Result<Pipeline, CompileError> {
    let lines = collect_lines(input);
    let jobs_index = find_line(&lines, 0, "jobs:").ok_or(CompileError::MissingJobs)?;
    let mut pending = Vec::new();
    let mut i = jobs_index + 1;
    while i < lines.len() {
        if lines[i].indent == 2 && is_yaml_map_header(&lines[i].text) {
            let origin = header_name(&lines[i].text);
            let mut j = i + 1;
            while j < lines.len() && !(lines[j].indent == 2 && is_yaml_map_header(&lines[j].text)) {
                j += 1;
            }
            pending.extend(parse_github_job(&origin, &lines[i + 1..j], context)?);
            i = j;
        } else {
            i += 1;
        }
    }
    finish_pipeline(PipelineSource::GitHubActions, pending, context)
}

fn parse_github_job(
    origin: &str,
    block: &[SourceLine],
    context: &CompileContext,
) -> Result<Vec<PendingJob>, CompileError> {
    let mut runner = context.default_runner.clone();
    let mut needs = Vec::new();
    let mut timeout_seconds = 3600;
    let mut steps = Vec::new();
    let mut matrix: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut token_scope = TokenScope::ReadRepo;
    let mut cache_mounts = Vec::new();
    let mut artifact_paths = Vec::new();

    let mut i = 0;
    while i < block.len() {
        let line = &block[i];
        if line.indent == 4 {
            if let Some(value) = scalar_after(&line.text, "runs-on:") {
                runner = RunnerClass::from_str(value)
                    .map_err(|_| CompileError::InvalidRunner(value.to_string()))?;
            } else if let Some(value) = scalar_after(&line.text, "needs:") {
                needs = parse_array_or_scalar(value);
            } else if line.text == "needs:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                needs = parse_yaml_list(slice);
                i = next;
                continue;
            } else if let Some(value) = scalar_after(&line.text, "timeout-minutes:") {
                // A malformed `timeout-minutes:` defaults to 60 minutes: GitHub
                // Actions treats the field as advisory with a 60-minute default,
                // so the invalid case is the intended fallback, not a hidden
                // error.
                timeout_seconds = value.trim().parse::<u64>().unwrap_or(60).saturating_mul(60);
            } else if let Some(value) = scalar_after(&line.text, "permissions:") {
                token_scope = parse_token_scope(value);
            } else if line.text == "steps:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                steps = parse_steps(origin, slice)?;
                i = next;
                continue;
            } else if line.text == "strategy:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                matrix = parse_matrix(slice);
                i = next;
                continue;
            } else if line.text == "cache:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                cache_mounts.extend(parse_cache_mounts(slice, origin));
                i = next;
                continue;
            } else if line.text == "artifacts:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                artifact_paths.extend(parse_artifacts(slice, origin));
                i = next;
                continue;
            }
        }
        i += 1;
    }

    if steps.is_empty() {
        return Err(CompileError::MissingSteps(origin.to_string()));
    }

    let combos = matrix_combinations(&matrix);
    let mut jobs = Vec::new();
    for combo in combos {
        let id = expanded_id(origin, &combo);
        let mut job = Job::new(&id, expanded_name(origin, &combo), runner.clone());
        job.steps = steps_with_matrix(&steps, &combo, &id);
        job.inputs = combo
            .iter()
            .map(|(k, v)| (format!("matrix.{k}"), v.clone()))
            .collect();
        job.timeout_seconds = timeout_seconds;
        job.token_scope = token_scope.clone();
        job.cache_mounts = if cache_mounts.is_empty() {
            default_cache_mounts(&id)
        } else {
            cache_mounts.clone()
        };
        job.artifact_paths = artifact_paths.clone();
        jobs.push(PendingJob {
            origin: origin.to_string(),
            job,
            needs: needs.clone(),
        });
    }
    Ok(jobs)
}
