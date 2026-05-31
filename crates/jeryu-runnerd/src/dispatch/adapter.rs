//! Protocol-to-core job adaptation: validate a runner protocol job and translate
//! it into the executable [`CoreJobRequest`] shape, failing closed on anything
//! jeryu_runner_core cannot faithfully preserve.

use jeryu_runner_core::JobRequest as CoreJobRequest;
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::trust::RunnerClass;
use jeryu_runner_protocol::JobRequest as ProtocolJobRequest;
use std::collections::BTreeMap;

use super::ProtocolDispatchContext;

/// Convert a runner protocol job into jeryu_runner_core's executable job shape.
pub fn protocol_to_core_job(
    request: &ProtocolJobRequest,
    context: &ProtocolDispatchContext,
) -> RunnerResult<CoreJobRequest> {
    validate_protocol_identity(request)?;
    if !request.cache_mounts.is_empty() {
        return adapter_error("cache mounts are not supported by jeryu_runner_core jobs yet");
    }
    if !request.artifact_paths.is_empty() {
        return adapter_error("artifact paths are not supported by jeryu_runner_core jobs yet");
    }
    if request.steps.len() != 1 {
        return adapter_error(format!(
            "expected exactly one executable step, got {}",
            request.steps.len()
        ));
    }

    let step = &request.steps[0];
    if step.uses.is_some() {
        return adapter_error("action-style steps are not executable by jeryu_runnerd");
    }
    if step.working_directory.is_some() {
        return adapter_error("step working directories are not supported by jeryu_runnerd");
    }
    let Some(command) = step
        .command
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    else {
        return adapter_error("protocol step is missing an executable command");
    };
    if command.contains('\0') {
        return adapter_error("protocol step command contains NUL byte");
    }

    let mut env = request.env.clone();
    merge_step_env(&mut env, &step.env)?;
    validate_env(&env)?;

    let timeout_ms = request
        .timeout_seconds
        .checked_mul(1000)
        .ok_or_else(|| adapter_runner_error("timeout_seconds overflows timeout_ms"))?;
    if timeout_ms == 0 {
        return adapter_error("timeout_seconds must be positive");
    }

    let job = CoreJobRequest {
        job_id: request.job_id.clone(),
        repo_id: context.repo_id.clone(),
        commit_sha: context.commit_sha.clone(),
        workspace: context.workspace.clone(),
        command: "/bin/sh".to_string(),
        args: vec!["-lc".to_string(), command.clone()],
        env,
        trust_tier: context.trust_tier,
        requested_runner: Some(map_runner_class(&request.runner_class)?),
        network_policy: context.network_policy,
        secret_policy: context.secret_policy,
        token_policy: context.token_policy,
        timeout_ms,
        fork: context.fork,
    };
    job.validate()?;
    select_runner(&job)?;
    Ok(job)
}

fn validate_protocol_identity(request: &ProtocolJobRequest) -> RunnerResult<()> {
    for (field, value) in [
        ("request_id", request.request_id.as_str()),
        ("pipeline_id", request.pipeline_id.as_str()),
        ("run_id", request.run_id.as_str()),
        ("lease_id", request.lease_id.as_str()),
        ("job_id", request.job_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return adapter_error(format!("protocol field '{field}' cannot be blank"));
        }
    }

    let expected_request_id = jeryu_ci_ir::deterministic_hash(&format!(
        "job-request|{}|{}|{}|{}",
        request.pipeline_id, request.run_id, request.lease_id, request.job_id
    ));
    if request.request_id != expected_request_id {
        return adapter_error("protocol request_id does not match stable identity fields");
    }
    Ok(())
}

fn map_runner_class(class: &jeryu_ci_ir::RunnerClass) -> RunnerResult<RunnerClass> {
    match class {
        jeryu_ci_ir::RunnerClass::NativeRustHot => Ok(RunnerClass::NativeRustHot),
        jeryu_ci_ir::RunnerClass::NativeRustClean => Ok(RunnerClass::NativeRustClean),
        jeryu_ci_ir::RunnerClass::AgentGuard => Ok(RunnerClass::AgentGuard),
        jeryu_ci_ir::RunnerClass::ReleaseHermetic => Ok(RunnerClass::ReleaseHermetic),
        jeryu_ci_ir::RunnerClass::MicrovmRust => Ok(RunnerClass::MicroVmRust),
        jeryu_ci_ir::RunnerClass::OciDocker => Ok(RunnerClass::OciDocker),
        unsupported => adapter_error(format!(
            "unsupported protocol runner class '{}'",
            unsupported.as_str()
        )),
    }
}

fn merge_step_env(
    target: &mut BTreeMap<String, String>,
    step_env: &BTreeMap<String, String>,
) -> RunnerResult<()> {
    for (key, value) in step_env {
        if let Some(existing) = target.get(key) {
            if existing != value {
                return adapter_error(format!("conflicting environment value for '{key}'"));
            }
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

fn validate_env(env: &BTreeMap<String, String>) -> RunnerResult<()> {
    for key in env.keys() {
        let valid = !key.is_empty()
            && key
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
            && !key.chars().next().is_some_and(|ch| ch.is_ascii_digit());
        if !valid {
            return adapter_error(format!("invalid environment variable '{key}'"));
        }
        if is_denied_env_key(key) {
            return adapter_error(format!("ambient environment variable '{key}' is denied"));
        }
    }
    Ok(())
}

fn is_denied_env_key(key: &str) -> bool {
    matches!(
        key,
        "SSH_AUTH_SOCK"
            | "AWS_ACCESS_KEY_ID"
            | "AWS_SECRET_ACCESS_KEY"
            | "AWS_SESSION_TOKEN"
            | "GOOGLE_APPLICATION_CREDENTIALS"
            | "GITHUB_TOKEN"
    )
}

fn adapter_error<T>(message: impl Into<String>) -> RunnerResult<T> {
    Err(adapter_runner_error(message))
}

fn adapter_runner_error(message: impl Into<String>) -> RunnerError {
    RunnerError::new("protocol_adapter_denied", message)
}
