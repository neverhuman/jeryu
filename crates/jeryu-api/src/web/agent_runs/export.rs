use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::CreatePullRequestRequest;
use serde::Serialize;

use crate::web::WebState;
use crate::web::workcells_support::{TypedError, forge_error, parse_json_body, typed_error};

use super::EMPTY_TREE_SHA;
use super::errors::{agent_run_not_found, agent_typed_error};
use super::git::{
    current_head_sha, derive_allowed_prefixes, freeze_workspace, git_branch_force, git_diff_names,
};
use super::source::{default_export_branch, normalize_pr_base};
use super::state::AgentRunPhase;
use super::types::AgentExportPrRequest;

#[derive(Debug, Clone, Serialize)]
struct AgentRunExportResponse {
    agent_run_id: String,
    branch: String,
    target_branch: String,
    pull_request_number: u64,
    url: String,
}

pub(in crate::web) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(agent_run_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: AgentExportPrRequest = match parse_json_body(
        &body,
        "export an agent-edit run into a pull request",
        "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };

    let mut runs = state.agent_runs.lock().expect("agent-run manager lock");
    let Some(snapshot) = runs.get(&agent_run_id) else {
        return agent_run_not_found(&agent_run_id);
    };
    if !matches!(
        snapshot.phase,
        AgentRunPhase::Exited | AgentRunPhase::Failed | AgentRunPhase::Terminated
    ) {
        return agent_typed_error(
            StatusCode::CONFLICT,
            "agent_run_not_finished",
            "export an agent-edit run into a pull request",
            "the run must finish before export can freeze the diff",
            &[
                "wait for the run to exit or terminate",
                "reload the agent-run status before retrying the export",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        );
    }

    let git_bin = state.repo_manager.config().git_bin.clone();
    if let Err(response) = freeze_workspace(&git_bin, &snapshot.source_root) {
        return *response;
    }
    let head_sha = current_head_sha(&git_bin, &snapshot.source_root)
        .unwrap_or_else(|| EMPTY_TREE_SHA.to_string());
    let changed_files = match git_diff_names(
        &git_bin,
        &snapshot.source_root,
        &snapshot.base_sha,
        &head_sha,
    ) {
        Ok(changed_files) => changed_files,
        Err(response) => return *response,
    };
    let allowed_prefixes =
        match derive_allowed_prefixes(&snapshot.allowed_paths, &snapshot.source_root) {
            Ok(prefixes) => prefixes,
            Err(response) => return *response,
        };
    let changed_files = match jeryu_codegraph::enforce_export_slice_from_diff(
        &changed_files,
        &allowed_prefixes,
    ) {
        Ok(changed_files) => changed_files,
        Err(denied) => {
            let message = match denied.git_error {
                Some(git_error) => {
                    format!("the export slice gate could not verify the diff: {git_error}")
                }
                None => format!(
                    "the export changed files outside the run slice: {}",
                    denied.out_of_slice_paths.join(", ")
                ),
            };
            return typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "agent_run_export_slice_denied",
                purpose: "export an agent-edit run into a pull request",
                reason: &message,
                common_fixes: &[
                    "restrict the agent edits to files inside the run's allowed paths",
                    "rerun the run after repairing the workspace slice",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                message: &message,
            });
        }
    };

    let branch = snapshot
        .export_branch
        .clone()
        .unwrap_or_else(|| default_export_branch(&snapshot));
    if let Err(response) = git_branch_force(&git_bin, &snapshot.source_root, &branch, &head_sha) {
        return *response;
    }

    let target_branch = normalize_pr_base(snapshot.base_ref.clone());
    let pr = match state.core.create_pull_request(
        &snapshot.owner,
        &snapshot.repo,
        &snapshot.agent,
        CreatePullRequestRequest {
            title: request.title,
            body: request.body,
            head: branch.clone(),
            base: target_branch.clone(),
            head_sha: Some(head_sha.clone()),
            base_sha: Some(snapshot.base_sha.clone()),
            source_repository: Some(format!("{}/{}", snapshot.owner, snapshot.repo)),
            draft: false,
            commits: Vec::new(),
            changed_files,
        },
    ) {
        Ok(pr) => pr,
        Err(err) => return forge_error(err),
    };

    let response = AgentRunExportResponse {
        agent_run_id: agent_run_id.clone(),
        branch: branch.clone(),
        target_branch: target_branch.clone(),
        pull_request_number: pr.number,
        url: format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
    };

    let _ = runs.update(&agent_run_id, |run| {
        run.phase = AgentRunPhase::Exported;
        run.head_sha = head_sha;
        run.export_branch = Some(branch);
        run.export_pull_request_number = Some(pr.number);
        run.error = None;
    });

    (StatusCode::CREATED, Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Bytes;
    use axum::extract::{Path as AxumPath, State};
    use axum::http::StatusCode;
    use jeryu_core::ForgeCore;

    use super::*;
    use crate::web::agent_runs::state::{AgentRunPhase, AgentRunSnapshot};

    fn snapshot(run_id: &str) -> AgentRunSnapshot {
        AgentRunSnapshot {
            agent_run_id: run_id.to_string(),
            workcell_id: "wc-export".to_string(),
            runner_id: "runner-export".to_string(),
            runner_epoch: 7,
            phase: AgentRunPhase::Running,
            agent: "codex".to_string(),
            model: "gpt-5.4-mini".to_string(),
            prompt: "fix".to_string(),
            source_kind: "scratch".to_string(),
            source_root: PathBuf::from("/tmp/jeryu-agent-export"),
            owner: "local".to_string(),
            repo: "demo".to_string(),
            base_ref: "main".to_string(),
            branch_suffix: "agent-edit".to_string(),
            allowed_paths: vec!["/tmp/jeryu-agent-export/src".to_string()],
            base_sha: "base".to_string(),
            head_sha: "base".to_string(),
            status_url: format!("/api/v1/agent-runs/{run_id}"),
            control_topic: "jeryu.agent.control.v1".to_string(),
            tty_topic: "jeryu.agent.tty.v1".to_string(),
            export_pr_url: format!("/api/v1/agent-runs/{run_id}/export_pr"),
            events: Vec::new(),
            controls: Vec::new(),
            outcome: None,
            error: None,
            export_pull_request_number: None,
            export_branch: Some("agents/codex/wc-export/agent-edit".to_string()),
        }
    }

    #[tokio::test]
    async fn export_rejects_unknown_or_unfinished_runs_before_git() {
        let git_root = std::env::temp_dir().join(format!(
            "jeryu-agent-export-git-{}",
            crate::web::agent_runs::now_millis()
        ));
        let state = Arc::new(crate::web::WebState::new_with_git_storage(
            ForgeCore::new(),
            git_root.clone(),
        ));
        let body = Bytes::from_static(br#"{"title":"export"}"#);
        let missing = export_pr(
            State(state.clone()),
            AxumPath("missing".to_string()),
            body.clone(),
        )
        .await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        state
            .agent_runs
            .lock()
            .unwrap()
            .insert(snapshot("ar-export"));
        let unfinished = export_pr(State(state), AxumPath("ar-export".to_string()), body).await;
        assert_eq!(unfinished.status(), StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(git_root);
    }
}
