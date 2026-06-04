use std::collections::BTreeMap;
use std::path::PathBuf;

use axum::http::StatusCode;
use axum::response::Response as AxumResponse;
use jeryu_agent_auth::{AgentAuthError, AgentAuthRepair};
use jeryu_agent_stream::BrokerConfig;

use super::errors::{agent_typed_error, auth_error, stream_error};
use super::types::{AgentRunSource, AgentWorkRequest};

type PreflightResult<T> = Result<T, Box<AxumResponse>>;

pub(super) fn verify_launch_preflight(
    request: &AgentWorkRequest,
    env: &BTreeMap<String, String>,
) -> PreflightResult<()> {
    if request.stream.required
        && let Err(err) = BrokerConfig::from_env(env)
    {
        return Err(Box::new(stream_error(err)));
    }

    let data_home = match env.get("JERYU_AGENT_AUTH_DATA_HOME") {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => return Err(Box::new(auth_error(missing_auth_data_home(request)))),
    };
    match jeryu_agent_auth::doctor(&data_home, request.agent) {
        Ok(report) if report.ok => {}
        Ok(_) => return Err(Box::new(auth_error(missing_imported_auth(request)))),
        Err(err) => return Err(Box::new(auth_error(err))),
    }

    verify_tool_path(request, env)?;
    verify_netguard(env)?;
    verify_sandbox(env)?;
    Ok(())
}

pub(super) fn validate_request(request: &AgentWorkRequest) -> PreflightResult<()> {
    let invalid = request.prompt.trim().is_empty()
        || request.model.trim().is_empty()
        || request.base_ref.trim().is_empty()
        || request.effort.trim().is_empty()
        || request.branch_suffix.trim().is_empty()
        || request.allowed_paths.is_empty()
        || request.budget.wall_secs == 0
        || request.budget.output_bytes == 0;
    if invalid {
        return Err(Box::new(agent_typed_error(
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
        )));
    }
    match &request.source {
        AgentRunSource::Repo { repo } if repo.trim().is_empty() => {
            Err(Box::new(agent_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "agent_run_invalid_request",
                "validate agent-edit run source",
                "repo source must name owner/repo",
                &["send a repo source with owner/name"],
                "docs/testing.md#workcells",
                "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            )))
        }
        AgentRunSource::LocalPath { local_path } if local_path.as_os_str().is_empty() => {
            Err(Box::new(agent_typed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "agent_run_invalid_request",
                "validate agent-edit run source",
                "local_path source must name a path",
                &["send a non-empty local_path"],
                "docs/testing.md#workcells",
                "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            )))
        }
        _ => Ok(()),
    }
}

fn verify_tool_path(
    request: &AgentWorkRequest,
    env: &BTreeMap<String, String>,
) -> PreflightResult<()> {
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
    Err(Box::new(agent_typed_error(
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
    )))
}

fn verify_netguard(env: &BTreeMap<String, String>) -> PreflightResult<()> {
    if env
        .get("JERYU_AGENT_EGRESS_PROXY")
        .filter(|value| !value.trim().is_empty())
        .is_some()
        && env.get("JERYU_AGENT_NETGUARD_ATTACHED").map(String::as_str) == Some("1")
    {
        return Ok(());
    }
    Err(Box::new(agent_typed_error(
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
    )))
}

fn verify_sandbox(env: &BTreeMap<String, String>) -> PreflightResult<()> {
    if env.get("JERYU_AGENT_SANDBOX_ENFORCED").map(String::as_str) == Some("1") {
        return Ok(());
    }
    Err(Box::new(agent_typed_error(
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
    )))
}

fn missing_auth_data_home(request: &AgentWorkRequest) -> AgentAuthError {
    AgentAuthError {
        code: "agent_auth_missing".to_string(),
        repair: Box::new(AgentAuthRepair {
            purpose: format!("verify imported {} auth", request.agent),
            reason: "JERYU_AGENT_AUTH_DATA_HOME is not configured".to_string(),
            common_fixes: vec![
                "run jeryu agent auth import --from-host for the requested tool".to_string(),
                "set JERYU_AGENT_AUTH_DATA_HOME to the Jeryu agent-auth data root".to_string(),
            ],
            docs_url: "docs/testing.md#workcells".to_string(),
            repair_hint: "rerun cargo test -p jeryu-agent-auth --jobs 40".to_string(),
        }),
    }
}

fn missing_imported_auth(request: &AgentWorkRequest) -> AgentAuthError {
    AgentAuthError {
        code: "agent_auth_missing".to_string(),
        repair: Box::new(AgentAuthRepair {
            purpose: format!("verify imported {} auth", request.agent),
            reason: format!("no imported {} auth files were found", request.agent),
            common_fixes: vec![
                "run jeryu agent auth import --from-host for the requested tool".to_string(),
                "confirm the imported files are under agent-auth/<tool>/".to_string(),
            ],
            docs_url: "docs/testing.md#workcells".to_string(),
            repair_hint: "rerun cargo test -p jeryu-agent-auth --jobs 40".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use jeryu_agent_auth::AgentToolKind;

    use super::*;
    use crate::web::agent_runs::types::{AgentRunBudget, AgentRunSource, AgentRunStreamOptions};

    fn request() -> AgentWorkRequest {
        AgentWorkRequest {
            source: AgentRunSource::Scratch {
                name: Some("demo".to_string()),
            },
            agent: AgentToolKind::Codex,
            prompt: "fix it".to_string(),
            model: "gpt-5.4-mini".to_string(),
            base_ref: "main".to_string(),
            effort: "xhigh".to_string(),
            allowed_paths: vec!["src".to_string()],
            branch_suffix: "agent-edit".to_string(),
            budget: AgentRunBudget {
                wall_secs: 60,
                output_bytes: 1024,
            },
            stream: AgentRunStreamOptions { required: false },
        }
    }

    fn unique_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "jeryu-agent-preflight-{tag}-{}",
            crate::web::agent_runs::now_millis()
        ))
    }

    fn env_with_auth() -> (BTreeMap<String, String>, PathBuf) {
        let data_home = unique_dir("auth");
        let auth_dir = data_home.join("agent-auth/codex");
        std::fs::create_dir_all(&auth_dir).unwrap();
        std::fs::write(auth_dir.join("auth.json"), "{}").unwrap();

        let mut env = BTreeMap::new();
        env.insert(
            "JERYU_AGENT_AUTH_DATA_HOME".to_string(),
            data_home.display().to_string(),
        );
        env.insert(
            "JERYU_AGENT_TOOL_CODEX_PATH".to_string(),
            "/bin/echo".to_string(),
        );
        env.insert(
            "JERYU_AGENT_EGRESS_PROXY".to_string(),
            "127.0.0.1:19090".to_string(),
        );
        env.insert("JERYU_AGENT_NETGUARD_ATTACHED".to_string(), "1".to_string());
        env.insert("JERYU_AGENT_SANDBOX_ENFORCED".to_string(), "1".to_string());
        (env, data_home)
    }

    #[test]
    fn validate_request_rejects_empty_required_fields_and_sources() {
        let mut req = request();
        req.prompt.clear();
        assert!(validate_request(&req).is_err());

        let mut req = request();
        req.source = AgentRunSource::Repo {
            repo: String::new(),
        };
        assert!(validate_request(&req).is_err());

        let mut req = request();
        req.source = AgentRunSource::LocalPath {
            local_path: PathBuf::new(),
        };
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn verify_launch_preflight_fails_closed_until_all_evidence_exists() {
        let req = request();
        assert!(verify_launch_preflight(&req, &BTreeMap::new()).is_err());

        let (mut env, data_home) = env_with_auth();
        assert!(verify_launch_preflight(&req, &env).is_ok());

        env.remove("JERYU_AGENT_TOOL_CODEX_PATH");
        assert!(verify_launch_preflight(&req, &env).is_err());
        env.insert(
            "JERYU_AGENT_TOOL_CODEX_PATH".to_string(),
            "/bin/echo".to_string(),
        );

        env.remove("JERYU_AGENT_NETGUARD_ATTACHED");
        assert!(verify_launch_preflight(&req, &env).is_err());
        env.insert("JERYU_AGENT_NETGUARD_ATTACHED".to_string(), "1".to_string());

        env.remove("JERYU_AGENT_SANDBOX_ENFORCED");
        assert!(verify_launch_preflight(&req, &env).is_err());

        let _ = std::fs::remove_dir_all(data_home);
    }

    #[test]
    fn required_stream_without_broker_config_is_a_typed_denial() {
        let mut req = request();
        req.stream.required = true;
        assert!(verify_launch_preflight(&req, &BTreeMap::new()).is_err());
    }
}
