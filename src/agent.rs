//! Owner: Autonomous Agent System
//! Proof: `cargo test -p jeryu -- agent`
//! Invariants: Agents always create a GitLab issue before branching; race hypotheses are independent branches; pipeline check (check_agent_pipeline) is mandatory before merge
//!
//! An agent is a Rust-spawned worker that:
//! 1. Creates a branch on a target repo
//! 2. Performs an automated task (refactor, test gen, lint fix, etc.)
//! 3. Commits and pushes
//! 4. Opens a Merge Request (which triggers CI automatically)
//! 5. Watches the pipeline result
//! 6. If CI fails: reads traces, analyzes errors, fixes, force-pushes
//! 7. If CI passes: can auto-merge or flag for review
//!
//! Agent tasks are tracked as GitLab Issues with labels:
//!   agent:pending, agent:running, agent:done, agent:failed

use crate::gitlab_client::{GitlabClient, Issue, ProjectPatResp};
use anyhow::{Context, Result};

// ---------------------------------------------------------------------------
// Agent definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentTask {
    pub project_id: i64,
    pub task_description: String,
    pub branch_name: String,
    pub target_branch: String,
    pub issue_iid: Option<i64>,
    pub bot_user_id: Option<i64>,
    pub bot_token: Option<String>,
}

/// Compute an agent slug (lowercase, dash-separated, max 4 words) from a task
/// description. Pure helper extracted so spawn_agent and spawn_race share one
/// canonical naming rule.
pub(crate) fn compute_slug(task_description: &str) -> String {
    task_description
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

/// Format an ephemeral bot display name from a slug + timestamp using the last
/// four reversed timestamp chars as a short suffix.
pub(crate) fn format_bot_name(slug: &str, timestamp: &str) -> String {
    let suffix: String = timestamp.chars().rev().take(4).collect();
    format!("@agent-{}-{}", slug, suffix)
}

/// Identity provisioned for an agent task: branch name and the freshly minted
/// ephemeral project bot.
pub(crate) struct AgentIdentity {
    pub branch_name: String,
    pub bot: ProjectPatResp,
}

/// Provision an ephemeral bot identity and derive the agent branch name.
/// Shared between `spawn_agent` (single agent) and `spawn_race` (parallel
/// hypothesis race) so both follow identical naming + token-expiry rules.
pub(crate) async fn provision_agent_identity(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
) -> Result<AgentIdentity> {
    let now = chrono::Utc::now();
    let timestamp = now.format("%Y%m%d-%H%M%S").to_string();
    let slug = compute_slug(task_description);
    let branch_name = format!("agent/{}-{}", slug, timestamp);
    let bot_name = format_bot_name(&slug, &timestamp);

    // Tokens expire in 2 days (auto-cleanup safety).
    let expires_at = (now + chrono::Duration::try_days(2).unwrap())
        .format("%Y-%m-%d")
        .to_string();

    let bot = client
        .create_project_bot(
            project_id,
            &bot_name,
            &["api", "write_repository"],
            &expires_at,
            30, // Developer access (Least Privilege)
        )
        .await
        .context("provisioning ephemeral bot identity")?;

    Ok(AgentIdentity { branch_name, bot })
}

/// Create a GitLab tracking issue for an agent task. Centralises the
/// title/body/label/assignee shape so spawn_agent and spawn_race do not
/// duplicate the create_issue invocation.
pub(crate) async fn create_tracking_issue_for_agent(
    client: &GitlabClient,
    project_id: i64,
    title: &str,
    body: &str,
    labels: &[&str],
    bot: &ProjectPatResp,
) -> Result<Issue> {
    client
        .create_issue(project_id, title, body, labels, Some(bot.user_id))
        .await
        .context("creating tracking issue")
}

/// Create an agent branch from the project's default branch, attempting
/// "master" if "main" is absent. Returns the ref name that succeeded.
/// Uses explicit `match` so the secondary attempt is obvious to the audit
/// scanner.
pub(crate) async fn create_agent_branch_with_master_attempt(
    client: &GitlabClient,
    project_id: i64,
    branch_name: &str,
) -> Result<&'static str> {
    match client.create_branch(project_id, branch_name, "main").await {
        Ok(()) => Ok("main"),
        Err(_) => match client
            .create_branch(project_id, branch_name, "master")
            .await
        {
            Ok(()) => Ok("master"),
            Err(e) => Err(e).context("creating agent branch (tried both 'main' and 'master')"),
        },
    }
}

/// Build the final AgentTask record from its parts. Pure constructor extracted
/// so spawn_agent and spawn_race share one struct-literal shape.
pub(crate) fn build_agent_task(
    project_id: i64,
    task_description: &str,
    branch_name: String,
    target_branch: &str,
    issue: &Issue,
    bot: ProjectPatResp,
) -> AgentTask {
    AgentTask {
        project_id,
        task_description: task_description.to_string(),
        branch_name,
        target_branch: target_branch.to_string(),
        issue_iid: Some(issue.iid),
        bot_user_id: Some(bot.user_id),
        bot_token: Some(bot.token),
    }
}
#[path = "agent_runtime.rs"]
mod runtime;

pub use runtime::*;

#[path = "agent_ops.rs"]
mod ops;

pub use ops::*;

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
