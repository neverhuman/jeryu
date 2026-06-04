use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use jeryu_agent_auth::{AgentAuthError, AgentAuthRepair};
use jeryu_agent_stream::BrokerConfig;

use super::errors::{agent_typed_error, auth_error, stream_error};
use super::types::{AgentRunSource, AgentWorkRequest};

pub(super) fn verify_launch_preflight(
    request: &AgentWorkRequest,
    env: &BTreeMap<String, String>,
) -> Result<(), AxumResponse> {
    if request.stream.required
        && let Err(err) = BrokerConfig::from_env(env)
    {
        return Err(stream_error(err));
    }

    let data_home = match env.get("JERYU_AGENT_AUTH_DATA_HOME") {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => return Err(auth_error(missing_auth_data_home(request))),
    };
    match jeryu_agent_auth::doctor(&data_home, request.agent) {
        Ok(report) if report.ok => {}
        Ok(_) => return Err(auth_error(missing_imported_auth(request))),
        Err(err) => return Err(auth_error(err)),
    }

    verify_tool_path(request, env)?;
    verify_netguard(env)?;
    verify_sandbox(env)?;
    Ok(())
}

pub(super) fn validate_request(request: &AgentWorkRequest) -> Result<(), AxumResponse> {
    let invalid = request.prompt.trim().is_empty()
        || request.model.trim().is_empty()
        || request.base_ref.trim().is_empty()
        || request.effort.trim().is_empty()
        || request.branch_suffix.trim().is_empty()
        || request.allowed_paths.is_empty()
        || request.budget.wall_secs == 0
        || request.budget.output_bytes == 0;
    if invalid {
        return Err(agent_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_invalid_request",
            "validate agent-edit run request",
            "required agent-run fields were empty or zero",
            &[
                "send source, agent, prompt, model, and base_ref",
                "keep budget.wall_secs and budget.output_bytes positive",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        ));
    }
    match &request.source {
        AgentRunSource::Repo { repo } if repo.trim().is_empty() => Err(agent_typed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "agent_run_invalid_request",
            "validate agent-edit run source",
            "repo source must name owner/repo",
            &["send a repo source with owner/name"],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        )),
        AgentRunSource::LocalPath { local_path } if local_path.as_os_str().is_empty() => {
            Err(agent_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "agent_run_invalid_request",
                "validate agent-edit run source",
                "local_path source must name a path",
                &["send a non-empty local_path"],
                "docs/testing.md#workcells",
                "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            ))
        }
        _ => Ok(()),
    }
}

fn verify_tool_path(
    request: &AgentWorkRequest,
    env: &BTreeMap<String, String>,
) -> Result<(), AxumResponse> {
    let tool_path_key = format!(
        "JERYU_AGENT_TOOL_{}_PATH",
        request.agent.as_str().to_ascii_uppercase()
    );
    if env
        .get(&tool_path_key)
        .filter(|path| !path.trim().is_empty())
        .is_some()
    {
        return Ok(());
    }
    Err(agent_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        "agent_tool_missing",
        "verify requested agent-edit tool",
        format!("{tool_path_key} is not configured"),
        &[
            "install the native CLI named in agent/native-cli-manifest.toml",
            "run the agent-edit runner doctor before accepting work",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40",
    ))
}

fn verify_netguard(env: &BTreeMap<String, String>) -> Result<(), AxumResponse> {
    if env
        .get("JERYU_AGENT_EGRESS_PROXY")
        .filter(|value| !value.trim().is_empty())
        .is_some()
        && env.get("JERYU_AGENT_NETGUARD_ATTACHED").map(String::as_str) == Some("1")
    {
        return Ok(());
    }
    Err(agent_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        "agent_netguard_unavailable",
        "verify proxy-only egress guard",
        "agent egress proxy or cgroup connect guard proof is missing",
        &[
            "configure JERYU_AGENT_EGRESS_PROXY",
            "attach the cgroup-v2 connect guard and set JERYU_AGENT_NETGUARD_ATTACHED=1",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-egress --jobs 40",
    ))
}

fn verify_sandbox(env: &BTreeMap<String, String>) -> Result<(), AxumResponse> {
    if env.get("JERYU_AGENT_SANDBOX_ENFORCED").map(String::as_str) == Some("1") {
        return Ok(());
    }
    Err(agent_typed_error(
        StatusCode::FAILED_DEPENDENCY,
        "agent_sandbox_unavailable",
        "verify required sandbox enforcement",
        "cgroup, Landlock, and seccomp enforcement proof is missing",
        &[
            "run the sandbox capability doctor on the target runner",
            "do not launch agent jobs until cgroup-v2 resource caps are enforced",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-sandbox-linux --jobs 40",
    ))
}

fn missing_auth_data_home(request: &AgentWorkRequest) -> AgentAuthError {
    AgentAuthError {
        code: "agent_auth_missing".to_string(),
        repair: AgentAuthRepair {
            purpose: format!("verify imported {} auth", request.agent),
            reason: "JERYU_AGENT_AUTH_DATA_HOME is not configured".to_string(),
            common_fixes: vec![
                "run jeryu agent auth import --from-host for the requested tool".to_string(),
                "set JERYU_AGENT_AUTH_DATA_HOME to the Jeryu agent-auth data root".to_string(),
            ],
            docs_url: "docs/testing.md#workcells".to_string(),
            repair_hint: "rerun cargo test -p jeryu-agent-auth --jobs 40".to_string(),
        },
    }
}

fn missing_imported_auth(request: &AgentWorkRequest) -> AgentAuthError {
    AgentAuthError {
        code: "agent_auth_missing".to_string(),
        repair: AgentAuthRepair {
            purpose: format!("verify imported {} auth", request.agent),
            reason: format!("no imported {} auth files were found", request.agent),
            common_fixes: vec![
                "run jeryu agent auth import --from-host for the requested tool".to_string(),
                "confirm the imported files are under agent-auth/<tool>/".to_string(),
            ],
            docs_url: "docs/testing.md#workcells".to_string(),
            repair_hint: "rerun cargo test -p jeryu-agent-auth --jobs 40".to_string(),
        },
    }
}
