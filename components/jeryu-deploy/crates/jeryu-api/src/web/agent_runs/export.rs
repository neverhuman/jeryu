//! Frozen workcell-diff validation and pull-request export.

use super::*;
use crate::web::workcells_support;

pub(super) fn export_workcell_agent_run(
    state: &Arc<WebState>,
    agent_run_id: &str,
    request: AgentRunExportPrRequest,
    origin_base_url: &str,
) -> AgentRunResponseResult<AgentRunExportPrResponse> {
    let record = state
        .agent_runs
        .record(agent_run_id)
        .ok_or_else(|| Box::new(agent_run_not_found(agent_run_id)))?;
    if record.state == AgentRunState::Running {
        return Err(boxed_agent_run_typed_error(
            StatusCode::CONFLICT,
            "agent_run_not_finished",
            "export an agent run into a pull request",
            "the run must finish before export can freeze the diff",
            &[
                "wait for the run to exit before exporting",
                "reload /api/v1/agent-runs/{id} and retry with a terminal run",
            ],
            "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
        ));
    }
    let (workcell_id, runner_epoch) = match &record.source {
        AgentRunSourceSnapshot::Workcell {
            workcell_id,
            runner_epoch,
            ..
        } => (workcell_id.clone(), *runner_epoch),
        _ => {
            return Err(boxed_agent_run_typed_error(
                StatusCode::FAILED_DEPENDENCY,
                "agent_run_export_source_unavailable",
                "export an agent run into a pull request",
                "only workcell-backed agent runs can be exported by the current route",
                &[
                    "start the run from a held or repairing workcell",
                    "wire repository and scratch source materialization before exporting those sources",
                ],
                "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
            ));
        }
    };

    let (lease, branch) = {
        let mut manager = manager(state);
        let branch_suffix = request
            .branch_suffix
            .clone()
            .unwrap_or_else(|| format!("agent-run-{agent_run_id}"));
        let branch = match manager.export_repair_branch(&workcell_id, runner_epoch, branch_suffix) {
            Ok(branch) => branch,
            Err(err) => return Err(Box::new(workcells_support::workcell_error(err))),
        };
        let Some(lease) = manager.workcell(&workcell_id).cloned() else {
            return Err(Box::new(workcells_support::workcell_not_found(
                &workcell_id,
            )));
        };
        (lease, branch)
    };

    let target_branch = request
        .target_branch
        .clone()
        .or_else(|| lease.startup_main_ref.clone())
        .map(normalize_pr_base)
        .unwrap_or_else(|| "main".to_string());
    let snapshot = lease.frozen_snapshot.as_ref();
    let head_sha = snapshot
        .map(|snapshot| snapshot.head_sha.clone())
        .or_else(|| lease.startup_head_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let base_sha = snapshot
        .map(|snapshot| snapshot.base_sha.clone())
        .or_else(|| lease.startup_base_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let allowed_prefixes = derive_allowed_prefixes(&lease.allowed_paths, &lease.workspace_root);
    let bare_repo = match state
        .repo_manager
        .resolve_parts(&request.owner, &request.repo)
    {
        Ok(repository) => repository.path,
        Err(err) => return Err(Box::new(forge_error(ForgeError::Storage(err.to_string())))),
    };
    let git_bin = state.repo_manager.config().git_bin.clone();
    let changed_files = match jeryu_codegraph::enforce_export_slice(
        &base_sha,
        &head_sha,
        &git_bin,
        &bare_repo,
        &allowed_prefixes,
    ) {
        Ok(files) => files,
        Err(denied) => {
            let message = match denied.git_error {
                Some(git_error) => {
                    format!("the export slice gate could not verify the diff: {git_error}")
                }
                None => format!(
                    "the export changed files outside the agent-run slice: {}",
                    denied.out_of_slice_paths.join(", ")
                ),
            };
            return Err(Box::new(typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "agent_run_export_slice_denied",
                purpose: "export an agent run into a pull request",
                reason: &message,
                common_fixes: &[
                    "restrict the agent edits to files inside the workcell's allowed paths",
                    "reclaim the workcell with a lease that covers the changed files",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40 agent_runs",
                message: &message,
            })));
        }
    };
    let pr = match state.github.core().create_pull_request(
        &request.owner,
        &request.repo,
        &request.author,
        CreatePullRequestRequest {
            title: request.title,
            body: request.body,
            head: branch.clone(),
            base: target_branch.clone(),
            head_sha: Some(head_sha),
            base_sha: Some(base_sha),
            source_repository: Some(format!("{}/{}", request.owner, request.repo)),
            draft: false,
            commits: Vec::new(),
            changed_files,
        },
    ) {
        Ok(pr) => pr,
        Err(err) => return Err(Box::new(forge_error(err))),
    };
    crate::ci_bridge::seed_pull_request_head(
        state.github.core(),
        state.repo_manager.as_ref(),
        &request.owner,
        &request.repo,
        &format!("refs/heads/{}", pr.head.ref_name),
        &pr.head.sha,
        origin_base_url,
    );
    state.agent_runs.mark_exported(agent_run_id);
    Ok(AgentRunExportPrResponse {
        agent_run_id: agent_run_id.to_string(),
        branch,
        target_branch,
        pull_request_number: pr.number,
        url: format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
    })
}
