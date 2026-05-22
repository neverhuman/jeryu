use super::*;
use crate::capability_records::{BranchCapabilityGrantInput, CapabilityRepo};

#[allow(clippy::too_many_arguments)] // capability bridge: flat schema mirrors AgentIntent::ProposePatch
pub(crate) async fn propose_patch(
    project_id: i64,
    branch_name: String,
    base_ref: String,
    commit_message: String,
    modifications: Vec<FileModification>,
    mr_title: Option<String>,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(repo) = CapabilityRepo::open_default().await else {
        return err("state store unavailable");
    };
    if let Err(e) = client
        .create_branch(project_id, &branch_name, &base_ref)
        .await
    {
        return err(&format!("create_branch: {}", e));
    }

    let tuples: Vec<(&str, &str, &str)> = modifications
        .iter()
        .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
        .collect();

    let commit_sha = match client
        .commit_actions_with_sha(project_id, &branch_name, &commit_message, &tuples)
        .await
    {
        Ok(sha) => sha,
        Err(e) => return err(&format!("commit_actions: {}", e)),
    };

    let title = match mr_title {
        Some(t) => t,
        None => commit_message.clone(),
    };
    match client
        .create_merge_request(project_id, &branch_name, &base_ref, &title, "")
        .await
    {
        Ok(mr) => {
            let grant = repo
                .record_branch_capability_grant(BranchCapabilityGrantInput {
                    intent_type: "ProposePatch",
                    action_id: "propose_patch",
                    protocol_version: &ctx.protocol_version,
                    request_id: &ctx.request_id,
                    actor: &ctx.actor,
                    bridge_mode: ctx.bridge_mode,
                    project_id,
                    branch_name: &branch_name,
                    target_ref: Some(&base_ref),
                    new_sha: Some(&commit_sha),
                    intent_payload: serde_json::json!({
                    "project_id": project_id,
                    "branch": branch_name,
                    "base_ref": base_ref,
                    "commit_sha": commit_sha.clone(),
                    "mr_iid": mr.iid,
                    "mr_url": mr.web_url,
                    "files_changed": modifications.len(),
                    }),
                })
                .await
                .ok();
            CapabilityResponse {
                success: true,
                message: format!("MR !{} created on branch {}", mr.iid, branch_name),
                data: Some(serde_json::json!({
                    "branch": branch_name,
                    "mr_iid": mr.iid,
                    "mr_url": mr.web_url,
                    "grant_id": grant,
                })),
            }
        }
        Err(e) => err(&format!("create_merge_request: {}", e)),
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse::error(msg)
}

#[path = "capability_execute_support.rs"]
mod support;
pub(crate) use support::{fetch_capsule, race_patches, request_merge, run_tests};
