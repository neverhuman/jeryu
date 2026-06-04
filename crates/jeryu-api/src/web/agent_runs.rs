mod errors;
mod preflight;
mod types;

use std::collections::BTreeMap;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::response::Response as AxumResponse;

use super::WebState;
use super::workcells_support::parse_json_body;
use errors::{agent_run_not_found, runtime_not_wired};
use preflight::{validate_request, verify_launch_preflight};
use std::sync::Arc;
use types::{AgentControlRequest, AgentExportPrRequest, AgentWorkRequest};

pub(super) async fn start(State(_state): State<Arc<WebState>>, body: Bytes) -> AxumResponse {
    let request: AgentWorkRequest = match parse_json_body(
        &body,
        "start an agent-edit run",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    if let Err(response) = validate_request(&request) {
        return response;
    }
    let env: BTreeMap<String, String> = std::env::vars().collect();
    if let Err(response) = verify_launch_preflight(&request, &env) {
        return response;
    }
    runtime_not_wired()
}

pub(super) async fn status(
    State(_state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
) -> AxumResponse {
    agent_run_not_found(&agent_run_id)
}

pub(super) async fn control(
    State(_state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let _request: AgentControlRequest = match parse_json_body(
        &body,
        "send control to an agent-edit run",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    agent_run_not_found(&agent_run_id)
}

pub(super) async fn export_pr(
    State(_state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let _request: AgentExportPrRequest = match parse_json_body(
        &body,
        "export an agent-edit run as a pull request",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    agent_run_not_found(&agent_run_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use jeryu_agent_auth::AgentToolKind;

    use types::{
        AgentRunBudget, AgentRunSource, AgentRunStreamOptions, default_allowed_paths,
        default_branch_suffix, default_effort,
    };

    fn valid_request() -> AgentWorkRequest {
        AgentWorkRequest {
            source: AgentRunSource::Repo {
                repo: "alice/jeryu".to_string(),
            },
            agent: AgentToolKind::Codex,
            prompt: "fix the test".to_string(),
            model: "model-x".to_string(),
            base_ref: "main".to_string(),
            effort: default_effort(),
            allowed_paths: default_allowed_paths(),
            branch_suffix: default_branch_suffix(),
            budget: AgentRunBudget::default(),
            stream: AgentRunStreamOptions::default(),
        }
    }

    #[test]
    fn defaults_match_agent_run_contract() {
        let request: AgentWorkRequest = serde_json::from_value(serde_json::json!({
            "source": { "kind": "repo", "repo": "alice/jeryu" },
            "agent": "codex",
            "prompt": "fix",
            "model": "m",
            "base_ref": "main"
        }))
        .expect("request parses");
        assert_eq!(request.effort, "xhigh");
        assert_eq!(request.allowed_paths, vec![""]);
        assert_eq!(request.branch_suffix, "agent-edit");
        assert_eq!(request.budget.wall_secs, 7200);
        assert_eq!(request.budget.output_bytes, 20_971_520);
        assert!(request.stream.required);
    }

    #[test]
    fn missing_required_stream_is_first_fail_closed_preflight() {
        let request = valid_request();
        let err =
            verify_launch_preflight(&request, &BTreeMap::new()).expect_err("missing stream denied");
        let body = axum::body::to_bytes(err.into_body(), usize::MAX);
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let body = rt.block_on(body).expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(json["code"], "agent_stream_required_unavailable");
        for key in [
            "purpose",
            "reason",
            "common_fixes",
            "docs_url",
            "repair_hint",
        ] {
            assert!(json.get(key).is_some(), "missing repair field {key}");
        }
    }
}
