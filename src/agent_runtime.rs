use anyhow::Result;
use tracing::info;

use super::*;

#[path = "agent_runtime_merge.rs"]
mod merge;
pub use merge::{create_agent_mr, merge_agent_mr, spawn_race};

/// Finalize a linear (non-race) agent task: open the tracking issue with the
/// pending label, create the agent branch (main with secondary attempt on master), promote
/// the issue to running, and assemble the AgentTask record. Centralised so
/// `spawn_agent` is one statement and the issue/branch/task shape exists in
/// exactly one place.
async fn finalize_linear_agent_task(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
    branch_name: String,
    bot: ProjectPatResp,
) -> Result<AgentTask> {
    let title = format!("[Agent] {}", task_description);
    let body = format!(
        "Autonomous agent task.\n\n\
         **Task:** {}\n\
         **Branch:** `{}`\n\
         **Identity:** `{}`\n\
         **Status:** Pending\n\n\
         _This issue is managed by jeryu agent._",
        task_description, branch_name, bot.name
    );
    let issue = create_tracking_issue_for_agent(
        client,
        project_id,
        &title,
        &body,
        &["agent:pending"],
        &bot,
    )
    .await?;
    info!(
        project_id,
        issue_iid = issue.iid,
        branch = %branch_name,
        bot_id = bot.user_id,
        "agent spawned"
    );
    let _ = create_agent_branch_with_master_attempt(client, project_id, &branch_name).await?;
    client
        .update_issue_labels(project_id, issue.iid, &["agent:running"])
        .await
        .ok();
    Ok(build_agent_task(
        project_id,
        task_description,
        branch_name,
        "main",
        &issue,
        bot,
    ))
}

/// Spawn an autonomous agent as a background task.
///
/// This creates a GitLab issue to track the work, creates a branch,
/// and returns immediately. The actual work is done asynchronously.
pub async fn spawn_agent(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
) -> Result<AgentTask> {
    let AgentIdentity { branch_name, bot } =
        provision_agent_identity(client, project_id, task_description).await?;

    finalize_linear_agent_task(client, project_id, task_description, branch_name, bot).await
}
