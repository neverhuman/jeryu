use super::*;

#[path = "capability_execute.rs"]
mod execute;
#[path = "capability_inspect.rs"]
mod inspect;
#[path = "capability_request.rs"]
mod request;

pub(crate) async fn execute_intent(
    intent: AgentIntent,
    ctx: &CapabilityContext,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match intent {
        AgentIntent::FetchCapsule { job_id } => execute::fetch_capsule(job_id).await,
        AgentIntent::RunTests {
            project_id,
            target_ref,
            test_scope,
        } => execute::run_tests(project_id, target_ref, test_scope, ctx, client).await,
        AgentIntent::ProposePatch {
            project_id,
            branch_name,
            base_ref,
            commit_message,
            modifications,
            mr_title,
        } => {
            execute::propose_patch(
                project_id,
                branch_name,
                base_ref,
                commit_message,
                modifications,
                mr_title,
                ctx,
                client,
            )
            .await
        }
        AgentIntent::RacePatches {
            project_id,
            base_branch,
            commit_message,
            hypotheses,
        } => {
            execute::race_patches(
                project_id,
                base_branch,
                commit_message,
                hypotheses,
                ctx,
                client,
            )
            .await
        }
        AgentIntent::RequestMerge {
            project_id,
            mr_iid,
            source_branch,
            target_branch,
        } => execute::request_merge(project_id, mr_iid, source_branch, target_branch, client).await,
        AgentIntent::ExplainBlockers {
            entity_type,
            entity_id,
        } => inspect::explain_blockers(entity_type, entity_id, client).await,
        AgentIntent::GetSystemSnapshot => inspect::get_system_snapshot(client).await,
        AgentIntent::GetPipelineJobs {
            project_id,
            pipeline_id,
        } => inspect::get_pipeline_jobs(project_id, pipeline_id, client).await,
        AgentIntent::GetCiBottlenecks {
            project_id,
            ref_name,
            limit,
        } => inspect::get_ci_bottlenecks(project_id, ref_name, limit, client).await,
        AgentIntent::ListAllowedActions => inspect::list_allowed_actions(),
        AgentIntent::PlanValidation {
            project_id,
            ref_name,
            test_ids,
        } => inspect::plan_validation(project_id, ref_name, test_ids).await,
        AgentIntent::BugSubmit {
            report,
            idempotency_key,
        } => bug_submit(report, idempotency_key, ctx).await,
        AgentIntent::BugList {
            project,
            status,
            sort,
        } => bug_list(project, status, sort).await,
        AgentIntent::BugShow { bug_id } => bug_show(bug_id).await,
        AgentIntent::BugReady { project } => bug_ready(project).await,
        AgentIntent::BugUpdate {
            bug_id,
            status,
            severity,
            priority,
            component,
            owner,
        } => bug_update(bug_id, status, severity, priority, component, owner, ctx).await,
        AgentIntent::BugRecordAttempt { bug_id, attempt } => {
            bug_record_attempt(bug_id, attempt, ctx).await
        }
    }
}

async fn bug_repo() -> anyhow::Result<crate::db::bugtracker_repo::BugTrackerRepo> {
    let db = crate::state::Db::open().await?;
    Ok(crate::db::bugtracker_repo::BugTrackerRepo::new(db.pool()))
}

async fn bug_submit(
    report: crate::bugtracker::CanonicalBugReport,
    idempotency_key: Option<String>,
    ctx: &CapabilityContext,
) -> CapabilityResponse {
    match bug_repo().await {
        Ok(repo) => match repo
            .submit_bug(&report, idempotency_key.as_deref(), ctx.actor())
            .await
        {
            Ok(bug) => CapabilityResponse {
                success: true,
                message: format!("submitted {}", bug.id),
                data: Some(serde_json::json!(bug)),
            },
            Err(err) => CapabilityResponse::error(&err.to_string()),
        },
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

async fn bug_list(
    project: Option<String>,
    status: Option<String>,
    sort: Option<String>,
) -> CapabilityResponse {
    match bug_repo().await {
        Ok(repo) => {
            let status = match status
                .as_deref()
                .map(crate::bugtracker::BugStatus::parse)
                .transpose()
            {
                Ok(value) => value,
                Err(err) => return CapabilityResponse::error(&err.to_string()),
            };
            let sort = match crate::bugtracker::BugSort::parse(sort.as_deref().unwrap_or("rank")) {
                Ok(value) => value,
                Err(err) => return CapabilityResponse::error(&err.to_string()),
            };
            match repo
                .list_bugs(project.as_deref().filter(|p| *p != "all"), status, sort)
                .await
            {
                Ok(bugs) => CapabilityResponse {
                    success: true,
                    message: format!("{} bug(s)", bugs.len()),
                    data: Some(serde_json::json!(bugs)),
                },
                Err(err) => CapabilityResponse::error(&err.to_string()),
            }
        }
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

async fn bug_show(bug_id: String) -> CapabilityResponse {
    match bug_repo().await {
        Ok(repo) => match repo.show_bug(&bug_id).await {
            Ok(detail) => CapabilityResponse {
                success: true,
                message: detail.bug.title.clone(),
                data: Some(serde_json::json!(detail)),
            },
            Err(err) => CapabilityResponse::error(&err.to_string()),
        },
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

async fn bug_ready(project: Option<String>) -> CapabilityResponse {
    match bug_repo().await {
        Ok(repo) => match repo
            .ready_bugs(project.as_deref().filter(|p| *p != "all"))
            .await
        {
            Ok(bugs) => CapabilityResponse {
                success: true,
                message: format!("{} ready bug(s)", bugs.len()),
                data: Some(serde_json::json!(bugs)),
            },
            Err(err) => CapabilityResponse::error(&err.to_string()),
        },
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

async fn bug_update(
    bug_id: String,
    status: Option<String>,
    severity: Option<String>,
    priority: Option<String>,
    component: Option<String>,
    owner: Option<String>,
    ctx: &CapabilityContext,
) -> CapabilityResponse {
    let status = match status
        .as_deref()
        .map(crate::bugtracker::BugStatus::parse)
        .transpose()
    {
        Ok(value) => value,
        Err(err) => return CapabilityResponse::error(&err.to_string()),
    };
    let severity = match severity.as_deref().map(parse_bug_severity).transpose() {
        Ok(value) => value,
        Err(err) => return CapabilityResponse::error(&err.to_string()),
    };
    let priority = match priority.as_deref().map(parse_bug_priority).transpose() {
        Ok(value) => value,
        Err(err) => return CapabilityResponse::error(&err.to_string()),
    };
    match bug_repo().await {
        Ok(repo) => match repo
            .update_bug(
                &bug_id,
                status,
                severity,
                priority,
                component.as_deref(),
                owner.as_deref(),
                ctx.actor(),
            )
            .await
        {
            Ok(bug) => CapabilityResponse {
                success: true,
                message: format!("updated {}", bug.id),
                data: Some(serde_json::json!(bug)),
            },
            Err(err) => CapabilityResponse::error(&err.to_string()),
        },
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

async fn bug_record_attempt(
    bug_id: String,
    attempt: crate::bugtracker::BugAttemptInput,
    ctx: &CapabilityContext,
) -> CapabilityResponse {
    match bug_repo().await {
        Ok(repo) => match repo.record_attempt(&bug_id, &attempt, ctx.actor()).await {
            Ok(attempt) => CapabilityResponse {
                success: true,
                message: format!("recorded attempt {}", attempt.id),
                data: Some(serde_json::json!(attempt)),
            },
            Err(err) => CapabilityResponse::error(&err.to_string()),
        },
        Err(err) => CapabilityResponse::error(&err.to_string()),
    }
}

fn parse_bug_severity(input: &str) -> anyhow::Result<crate::bugtracker::BugSeverity> {
    match input {
        "S0" | "s0" => Ok(crate::bugtracker::BugSeverity::S0),
        "S1" | "s1" => Ok(crate::bugtracker::BugSeverity::S1),
        "S2" | "s2" => Ok(crate::bugtracker::BugSeverity::S2),
        "S3" | "s3" => Ok(crate::bugtracker::BugSeverity::S3),
        "S4" | "s4" => Ok(crate::bugtracker::BugSeverity::S4),
        other => anyhow::bail!("unknown severity '{other}'"),
    }
}

fn parse_bug_priority(input: &str) -> anyhow::Result<crate::bugtracker::BugPriority> {
    match input {
        "P0" | "p0" => Ok(crate::bugtracker::BugPriority::P0),
        "P1" | "p1" => Ok(crate::bugtracker::BugPriority::P1),
        "P2" | "p2" => Ok(crate::bugtracker::BugPriority::P2),
        "P3" | "p3" => Ok(crate::bugtracker::BugPriority::P3),
        "P4" | "p4" => Ok(crate::bugtracker::BugPriority::P4),
        other => anyhow::bail!("unknown priority '{other}'"),
    }
}

pub(crate) fn parse_capability_request(bytes: &[u8]) -> anyhow::Result<ParsedCapabilityRequest> {
    request::parse_capability_request(bytes)
}

pub(crate) fn validate_capability_request(
    parsed: ParsedCapabilityRequest,
) -> anyhow::Result<(AgentIntent, CapabilityContext)> {
    request::validate_capability_request(parsed)
}
