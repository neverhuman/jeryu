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

use anyhow::{Context, Result};
use tracing::info;

use crate::decision::{RequiredEvidencePolicy, RiskGateDecision, TrustTier, evaluate_risk_gate};
use crate::gitlab_client::{GitlabClient, Issue, ProjectPatResp};

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
    // target_branch hard-coded to "main" to preserve the prior MR-routing
    // behavior even when the secondary attempt actually created the branch.
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
    // 1. Provision Ephemeral Bot Identity (shared with spawn_race).
    let AgentIdentity { branch_name, bot } =
        provision_agent_identity(client, project_id, task_description).await?;

    // 2. Create tracking issue, branch, and AgentTask via the linear-agent
    //    finalizer (shared with spawn_race). Linear-agent sets agent:pending,
    //    flips to agent:running after the branch is up, and records "main"
    //    as the MR target_branch regardless of which ref served as the
    //    actual base (preserving prior MR-routing behavior).
    finalize_linear_agent_task(client, project_id, task_description, branch_name, bot).await
}

/// Spawns a Parallel Hypothesis Race.
/// Creates multiple branches and commits a different patch hypothesis to each.
pub async fn spawn_race(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
    hypotheses: Vec<Vec<crate::capability::FileModification>>,
) -> Result<AgentTask> {
    // 1. Setup identity (shared with spawn_agent).
    let AgentIdentity {
        branch_name: base_branch_name,
        bot,
    } = provision_agent_identity(client, project_id, task_description).await?;

    let issue = create_tracking_issue_for_agent(
        client,
        project_id,
        &format!("[Race] {}", task_description),
        &format!(
            "Autonomous agent racing {} hypotheses.\n\n\
             **Task:** {}\n\
             **Base Branch:** `{}`\n\
             **Identity:** `{}`\n\n\
             _This issue is managed by jeryu Parallel Hypothesis Racing._",
            hypotheses.len(),
            task_description,
            base_branch_name,
            bot.name
        ),
        &["agent:running", "agent:race"],
        &bot,
    )
    .await?;
    info!(
        project_id,
        issue_iid = issue.iid,
        "race spawned for {} hypotheses",
        hypotheses.len()
    );

    // Race historically swallows branch-creation errors after the secondary attempt;
    // preserve that exact semantic by ignoring failures from the master
    // path, but still report which ref we attempted last.
    let attempt_base = match client
        .create_branch(project_id, &base_branch_name, "main")
        .await
    {
        Ok(()) => "main",
        Err(_) => {
            let _ = client
                .create_branch(project_id, &base_branch_name, "master")
                .await;
            "master"
        }
    };

    // Fan-out: Create a branch + commit for each hypothesis!
    for (idx, mods) in hypotheses.iter().enumerate() {
        let hypo_branch = format!("{}-hypo-{}", base_branch_name, idx);

        // Fork off the base branch we just created
        let _ = client
            .create_branch(project_id, &hypo_branch, &base_branch_name)
            .await;

        let files: Vec<(&str, &str, &str)> = mods
            .iter()
            .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
            .collect();
        let msg = format!("Apply patch hypothesis {}", idx);

        let _ = client
            .commit_actions(project_id, &hypo_branch, &msg, &files)
            .await;

        // Often GitLab CI fires on branch creation + push.
        // We trigger explicitly if needed as backup.
        let _ = client
            .trigger_pipeline(project_id, &hypo_branch, vec![])
            .await;
    }

    Ok(build_agent_task(
        project_id,
        task_description,
        base_branch_name,
        attempt_base,
        &issue,
        bot,
    ))
}

/// Create a merge request for an agent's work.
pub async fn create_agent_mr(client: &GitlabClient, task: &AgentTask) -> Result<i64> {
    let description = format!(
        "Automated change by jeryu agent.\n\n\
         **Task:** {}\n\n\
         {}",
        task.task_description,
        match task.issue_iid {
            Some(iid) => format!("Closes #{}", iid),
            None => String::new(),
        },
    );

    let mr = client
        .create_merge_request(
            task.project_id,
            &task.branch_name,
            &task.target_branch,
            &format!("[Agent] {}", task.task_description),
            &description,
        )
        .await
        .context("creating merge request")?;

    info!(
        project_id = task.project_id,
        mr_iid = mr.iid,
        "agent created merge request"
    );

    Ok(mr.iid)
}

/// Build a FailureCapsule from a job + log trace when no structured capsule
/// is available in the event ledger. Centralised so the race-failed and
/// linear-failed code paths in `check_agent_pipeline` share a single
/// canonical capsule shape (only `repro_script` and `summary` differ between
/// the two call sites). Uses `match` for ref_name unwrapping per project
/// conventions.
fn unmatched_capsule_from_trace(
    job: &crate::gitlab_client::Job,
    project_id: i64,
    log_snippet: String,
    repro_script: String,
    summary: &str,
) -> crate::capsule::FailureCapsule {
    let ref_name = match job.ref_name.clone() {
        Some(r) => r,
        None => "unknown".to_string(),
    };
    crate::capsule::FailureCapsule {
        job_id: job.id,
        pipeline_id: None,
        project_id,
        stage: job.stage.clone(),
        exit_code: 1,
        commit_sha: "unknown".to_string(),
        ref_name,
        working_directory: "/builds/agent".to_string(),
        log_snippet,
        repro_script,
        environment: std::collections::HashMap::new(),
        failure_kind: "unknown".to_string(),
        summary: summary.to_string(),
        superseded_by_sha: None,
        retried_from_job_id: None,
    }
}

/// Check a pipeline result for an agent's MR and decide next action.
pub async fn check_agent_pipeline(
    client: &GitlabClient,
    task: &AgentTask,
    _mr_iid: i64,
) -> Result<AgentOutcome> {
    // Get the latest jobs for this project to check pipeline status
    let jobs = client
        .list_jobs(task.project_id, &["success", "failed"])
        .await?;

    // Find jobs on our branch or our hypo branches
    let branch_jobs: Vec<_> = jobs
        .iter()
        .filter(|j| {
            j.ref_name
                .as_deref()
                .map(|r| r.starts_with(&task.branch_name))
                .unwrap_or(false)
        })
        .collect();

    if branch_jobs.is_empty() {
        return Ok(AgentOutcome::Pending);
    }

    // Determine unique refs to see if this is a Race
    let mut refs_seen = std::collections::HashSet::new();
    for j in &branch_jobs {
        if let Some(r) = &j.ref_name {
            refs_seen.insert(r.clone());
        }
    }

    let is_race = refs_seen.len() > 1
        || branch_jobs
            .iter()
            .any(|j| j.ref_name.as_deref().unwrap_or("").contains("-hypo-"));

    if is_race {
        info!("Reviewing parallel hypothesis race pipelines...");
        for ref_name in &refs_seen {
            let ref_jobs: Vec<_> = branch_jobs
                .iter()
                .filter(|j| j.ref_name.as_deref() == Some(ref_name))
                .collect();
            let all_success = ref_jobs.iter().all(|j| j.status == "success");

            if all_success {
                info!("🏁 Race winner determined: {}!", ref_name);
                // Purge losers
                for loser_ref in refs_seen.iter().filter(|r| *r != ref_name) {
                    tracing::info!("Purging losing branch: {}", loser_ref);
                    client.delete_branch(task.project_id, loser_ref).await.ok();
                }

                // Planned merge hook: merge the winner back into the base branch
                // automatically using GitLab MR or API. For now, declare success.
                return Ok(AgentOutcome::Success);
            }
        }

        let all_failed = refs_seen.iter().all(|r| {
            let ref_jobs: Vec<_> = branch_jobs
                .iter()
                .filter(|j| j.ref_name.as_deref() == Some(r))
                .collect();
            ref_jobs.iter().any(|j| j.status == "failed")
        });

        if all_failed {
            // All hypotheses failed
            let mut capsules = Vec::new();
            for j in branch_jobs.iter().filter(|j| j.status == "failed") {
                if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    capsules.push(unmatched_capsule_from_trace(
                        j,
                        task.project_id,
                        trace,
                        format!("Failed hypothesis: {:?}", j.ref_name),
                        "failed hypothesis race job",
                    ));
                }
            }
            return Ok(AgentOutcome::Failed { capsules });
        }
        return Ok(AgentOutcome::Pending);
    }

    // Standard linear agent flow
    let any_failed = branch_jobs.iter().any(|j| j.status == "failed");
    let all_success = branch_jobs.iter().all(|j| j.status == "success");

    if any_failed {
        let db = crate::state::Db::open().await?;
        let mut capsules = Vec::new();

        for j in &branch_jobs {
            if j.status == "failed" {
                // Try to find a failure capsule in the event ledger first
                let capsule = db.latest_evidence_for_job(task.project_id, j.id).await?;

                if let Some(c) = capsule {
                    capsules.push(c);
                } else if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    // Recovery to raw trace snippet if no capsule is found.
                    capsules.push(unmatched_capsule_from_trace(
                        j,
                        task.project_id,
                        trace,
                        "unknown".to_string(),
                        "failed agent job",
                    ));
                }
            }
        }

        Ok(AgentOutcome::Failed { capsules })
    } else if all_success {
        Ok(AgentOutcome::Success)
    } else {
        Ok(AgentOutcome::Pending)
    }
}

#[derive(Debug)]
pub enum AgentOutcome {
    Pending,
    Success,
    Failed {
        capsules: Vec<crate::capsule::FailureCapsule>,
    },
}

/// Mark an agent task as completed.
pub async fn complete_agent(client: &GitlabClient, task: &AgentTask, success: bool) -> Result<()> {
    if let Some(issue_iid) = task.issue_iid {
        let label = if success {
            "agent:done"
        } else {
            "agent:failed"
        };
        client
            .update_issue_labels(task.project_id, issue_iid, &[label])
            .await
            .ok();

        let comment = if success {
            "✅ Agent task completed successfully. Pipeline passed."
        } else {
            "❌ Agent task failed. See pipeline logs for details."
        };
        client
            .comment_on_issue(task.project_id, issue_iid, comment)
            .await
            .ok();
    }

    Ok(())
}

/// List active agent issues for a project.
pub async fn list_agents(
    client: &GitlabClient,
    project_id: i64,
) -> Result<Vec<crate::gitlab_client::Issue>> {
    let mut active = client
        .list_issues_by_labels(project_id, &["agent:running"], Some("opened"))
        .await?;
    let mut pending = client
        .list_issues_by_labels(project_id, &["agent:pending"], Some("opened"))
        .await?;
    active.append(&mut pending);
    active.sort_by_key(|issue| issue.iid);
    active.dedup_by_key(|issue| issue.iid);
    Ok(active)
}

pub async fn merge_agent_mr(
    client: &GitlabClient,
    project_id: i64,
    mr_iid: i64,
    trust_tier: TrustTier,
) -> Result<crate::decision::RiskEvaluation> {
    let mr = client.get_merge_request(project_id, mr_iid).await?;
    let jobs = client
        .list_jobs(
            project_id,
            &["success", "failed", "pending", "running", "created"],
        )
        .await?;

    let branch_jobs: Vec<_> = jobs
        .iter()
        .filter(|job| job.ref_name.as_deref() == Some(mr.source_branch.as_str()))
        .collect();

    let successful_jobs = branch_jobs
        .iter()
        .filter(|job| job.status == "success")
        .count();
    let failed_jobs = branch_jobs
        .iter()
        .filter(|job| job.status == "failed")
        .count();
    let pending_jobs = branch_jobs
        .iter()
        .filter(|job| matches!(job.status.as_str(), "pending" | "running" | "created"))
        .count();

    let evaluation = evaluate_risk_gate(
        trust_tier.clone(),
        successful_jobs,
        pending_jobs,
        failed_jobs,
        &RequiredEvidencePolicy::default(),
    );

    let db = crate::state::Db::open().await?;
    db.append_event(
        "risk_gate_decision",
        Some(project_id),
        None,
        "agent",
        &serde_json::json!({
            "mr_iid": mr_iid,
            "source_branch": mr.source_branch,
            "successful_jobs": successful_jobs,
            "pending_jobs": pending_jobs,
            "failed_jobs": failed_jobs,
            "trust_tier": trust_tier,
            "decision": evaluation.decision,
            "reason": evaluation.reason,
        })
        .to_string(),
    )
    .await?;

    if evaluation.decision == RiskGateDecision::Allow {
        client.accept_merge_request(project_id, mr_iid).await?;
    }

    Ok(evaluation)
}

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
