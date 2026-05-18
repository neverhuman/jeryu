use super::AgentTask;
use crate::gitlab_client::{GitlabClient, Issue, Job};
use anyhow::Result;
use tracing::info;

#[derive(Debug)]
pub enum AgentOutcome {
    Pending,
    Success,
    Failed {
        capsules: Vec<crate::capsule::FailureCapsule>,
    },
}

fn unmatched_capsule_from_trace(
    job: &Job,
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
        requeued_from_job_id: None,
    }
}

/// Check a pipeline result for an agent's MR and decide next action.
pub async fn check_agent_pipeline(
    client: &GitlabClient,
    task: &AgentTask,
    _mr_iid: i64,
) -> Result<AgentOutcome> {
    let jobs = client
        .list_jobs(task.project_id, &["success", "failed"])
        .await?;

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
                for loser_ref in refs_seen.iter().filter(|r| *r != ref_name) {
                    tracing::info!("Purging losing branch: {}", loser_ref);
                    client.delete_branch(task.project_id, loser_ref).await.ok();
                }
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

    let any_failed = branch_jobs.iter().any(|j| j.status == "failed");
    let all_success = branch_jobs.iter().all(|j| j.status == "success");

    if any_failed {
        let db = crate::state::Db::open().await?;
        let mut capsules = Vec::new();

        for j in &branch_jobs {
            if j.status == "failed" {
                let capsule = db.latest_evidence_for_job(task.project_id, j.id).await?;

                if let Some(c) = capsule {
                    capsules.push(c);
                } else if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
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
pub async fn list_agents(client: &GitlabClient, project_id: i64) -> Result<Vec<Issue>> {
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
