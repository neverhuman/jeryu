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
mod tests {
    use super::{build_agent_task, compute_slug, format_bot_name};
    use crate::gitlab_client::{Issue, ProjectPatResp};

    fn fake_issue(iid: i64) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": iid * 1000,
            "iid": iid,
            "title": "test",
            "state": "opened",
            "labels": ["agent:pending"],
            "web_url": "https://example.com/issues/1",
        }))
        .expect("issue fixture")
    }

    fn fake_bot() -> ProjectPatResp {
        serde_json::from_value(serde_json::json!({
            "id": 7,
            "name": "@agent-fix-foo-0000",
            "token": "secret",
            "user_id": 42,
        }))
        .expect("bot fixture")
    }

    #[test]
    fn build_agent_task_threads_identity_through_unchanged() {
        // Smoke test: helper preserves the AgentTask shape (issue iid, bot
        // user_id, branch, target_branch) that spawn_agent and spawn_race
        // both depend on. Guards the deduplication boundary.
        let issue = fake_issue(123);
        let bot = fake_bot();
        let task = build_agent_task(
            99,
            "repair foo",
            "agent/repair-foo-x".to_string(),
            "main",
            &issue,
            bot,
        );
        assert_eq!(task.project_id, 99);
        assert_eq!(task.task_description, "repair foo");
        assert_eq!(task.branch_name, "agent/repair-foo-x");
        assert_eq!(task.target_branch, "main");
        assert_eq!(task.issue_iid, Some(123));
        assert_eq!(task.bot_user_id, Some(42));
        assert_eq!(task.bot_token.as_deref(), Some("secret"));
    }

    #[test]
    fn slug_strips_punctuation_lowercases_and_caps_at_four_words() {
        // Punctuation dropped, words joined with '-', lowercased, max 4 words.
        let slug = compute_slug("Fix the BROKEN build, please ASAP!");
        assert_eq!(slug, "fix-the-broken-build");
    }

    #[test]
    fn slug_handles_empty_and_punct_only_input() {
        assert_eq!(compute_slug(""), "");
        assert_eq!(compute_slug("!!!---???"), "");
    }

    #[test]
    fn bot_name_uses_reversed_last_four_timestamp_chars() {
        // Suffix is the timestamp reversed, first 4 chars of the reverse.
        // Timestamp "20260506-120000" -> reverse "000021-60506202" -> "0000".
        let name = format_bot_name("repair-foo", "20260506-120000");
        assert_eq!(name, "@agent-repair-foo-0000");

        // Distinct timestamps with distinct tails produce distinct suffixes.
        let other = format_bot_name("repair-foo", "20260506-120123");
        assert_eq!(other, "@agent-repair-foo-3210");
    }
}
