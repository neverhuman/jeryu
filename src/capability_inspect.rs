use super::*;

pub(crate) async fn explain_blockers(
    entity_type: String,
    entity_id: i64,
    _client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    match entity_type.as_str() {
        "job" => match db.latest_evidence_by_job_id(entity_id).await {
            Ok(Some(cap)) => CapabilityResponse {
                success: true,
                message: format!("job {entity_id} blockers explained"),
                data: Some(match serde_json::to_value(cap) {
                    Ok(value) => value,
                    Err(err) => serde_json::json!({
                        "serialization_error": err.to_string(),
                    }),
                }),
            },
            Ok(None) => err(&format!("no failure capsule found for job {entity_id}")),
            Err(e) => err(&format!("job blockers: {}", e)),
        },
        "release" => match db.recent_release_attempts(None, None, 20).await {
            Ok(attempts) => match attempts.iter().find(|attempt| attempt.id == entity_id) {
                Some(attempt) => CapabilityResponse {
                    success: true,
                    message: format!("release {entity_id} blockers explained"),
                    data: Some(match serde_json::to_value(attempt) {
                        Ok(value) => value,
                        Err(err) => serde_json::json!({
                            "serialization_error": err.to_string(),
                        }),
                    }),
                },
                None => err(&format!("no release attempt found for id {entity_id}")),
            },
            Err(e) => err(&format!("release blockers: {}", e)),
        },
        "merge" => {
            let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
            match db.count_selector_misses_since(&since).await {
                Ok(miss_count) => CapabilityResponse {
                    success: true,
                    message: format!("merge {entity_id} blockers explained"),
                    data: Some(serde_json::json!({
                        "entity_id": entity_id,
                        "selector_misses_30d": miss_count,
                        "summary": if miss_count > 0 {
                            "unrepaired test selector misses remain"
                        } else {
                            "no selector misses"
                        }
                    })),
                },
                Err(e) => err(&format!("merge blockers: {}", e)),
            }
        }
        other => err(&format!("unknown entity type '{other}'")),
    }
}

pub(crate) async fn get_system_snapshot(
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let pools = match db.list_pools().await {
        Ok(pools) => pools,
        Err(e) => return err(&format!("system snapshot pools: {}", e)),
    };
    let recent_jobs = match db.recent_job_events(10).await {
        Ok(events) => events
            .into_iter()
            .map(|event| {
                serde_json::json!({
                    "job_id": event.job_id,
                    "project_id": event.project_id,
                    "pipeline_id": event.pipeline_id,
                    "status": event.status,
                    "job_name": event.job_name,
                    "pool_name": event.pool_name,
                    "system_id": event.system_id,
                    "queued_duration": event.queued_duration,
                    "received_at": event.received_at,
                })
            })
            .collect::<Vec<_>>(),
        Err(e) => return err(&format!("system snapshot jobs: {}", e)),
    };
    let latest_release = match db.recent_release_attempts(None, None, 1).await {
        Ok(attempts) => attempts.into_iter().next().map(|attempt| {
            serde_json::json!({
                "id": attempt.id,
                "project_id": attempt.project_id,
                "ref_name": attempt.ref_name,
                "sha": attempt.sha,
                "version": attempt.version,
                "upstream_pipeline_id": attempt.upstream_pipeline_id,
                "upstream_status": attempt.upstream_status,
                "release_pipeline_id": attempt.release_pipeline_id,
                "release_pipeline_status": attempt.release_pipeline_status,
                "production_pipeline_id": attempt.production_pipeline_id,
                "production_pipeline_status": attempt.production_pipeline_status,
                "canary_status": attempt.canary_status,
                "canary_started_at": attempt.canary_started_at,
                "canary_finished_at": attempt.canary_finished_at,
                "canary_note": attempt.canary_note,
                "created_at": attempt.created_at,
                "updated_at": attempt.updated_at,
            })
        }),
        Err(e) => return err(&format!("system snapshot release: {}", e)),
    };
    let gitlab_ready = client.is_ready().await;
    CapabilityResponse {
        success: true,
        message: "system snapshot".into(),
        data: Some(serde_json::json!({
            "gitlab_ready": gitlab_ready,
            "pool_count": pools.len(),
            "recent_job_events": recent_jobs,
            "latest_release": latest_release,
        })),
    }
}

pub(crate) async fn get_pipeline_jobs(
    project_id: i64,
    pipeline_id: i64,
    client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    match client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await
    {
        Ok(jobs) => CapabilityResponse {
            success: true,
            message: "pipeline jobs".into(),
            data: Some(serde_json::json!({
                "project_id": project_id,
                "pipeline_id": pipeline_id,
                "jobs": jobs.into_iter().map(|job| {
                    serde_json::json!({
                        "id": job.id,
                        "name": job.name,
                        "status": job.status,
                        "stage": job.stage,
                        "allow_failure": job.allow_failure,
                        "ref_name": job.ref_name,
                        "web_url": job.web_url,
                        "queued_duration": job.queued_duration,
                        "duration": job.duration,
                        "started_at": job.started_at,
                        "finished_at": job.finished_at,
                        "runner": job.runner.and_then(|runner| runner.description),
                    })
                }).collect::<Vec<_>>()
            })),
        },
        Err(e) => err(&format!("pipeline jobs: {}", e)),
    }
}

pub(crate) async fn get_ci_bottlenecks(
    project_id: i64,
    ref_name: Option<String>,
    limit: Option<i64>,
    _client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let limit = limit.unwrap_or(20).max(1);
    match db
        .ci_job_bottlenecks(project_id, ref_name.as_deref(), limit)
        .await
    {
        Ok(rows) => CapabilityResponse {
            success: true,
            message: "ci bottlenecks".into(),
            data: Some(match serde_json::to_value(rows) {
                Ok(value) => value,
                Err(err) => serde_json::json!({
                    "serialization_error": err.to_string(),
                }),
            }),
        },
        Err(e) => err(&format!("ci bottlenecks: {}", e)),
    }
}

pub(crate) fn list_allowed_actions() -> CapabilityResponse {
    CapabilityResponse {
        success: true,
        message: "allowed actions".into(),
        data: Some(serde_json::json!({
            "actions": ["FetchCapsule", "RunTests", "ProposePatch", "RacePatches", "RequestMerge", "ExplainBlockers", "GetSystemSnapshot", "GetPipelineJobs", "GetCiBottlenecks", "ListAllowedActions", "PlanValidation"]
        })),
    }
}

pub(crate) async fn plan_validation(
    _project_id: i64,
    ref_name: String,
    test_ids: Vec<String>,
) -> CapabilityResponse {
    let Ok(db) = crate::state::Db::open().await else {
        return err("database unavailable");
    };
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(24))
        .unwrap_or(chrono::Utc::now())
        .to_rfc3339();
    let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);

    let valid = miss_count == 0;
    CapabilityResponse {
        success: valid,
        message: if valid {
            format!(
                "Plan for ref '{}' with {} tests is valid",
                ref_name,
                test_ids.len()
            )
        } else {
            format!(
                "Plan invalid: {} unresolved selector miss(es) in last 24h",
                miss_count
            )
        },
        data: Some(serde_json::json!({
            "valid": valid,
            "test_count": test_ids.len(),
            "ref_name": ref_name,
            "selector_misses": miss_count,
        })),
    }
}

fn err(msg: &str) -> CapabilityResponse {
    CapabilityResponse::error(msg)
}
