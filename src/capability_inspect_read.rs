use super::*;
use crate::capability_records::CapabilityRepo;

pub(crate) async fn explain_blockers(
    entity_type: String,
    entity_id: i64,
    _client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(repo) = CapabilityRepo::open_default().await else {
        return err("state store unavailable");
    };
    match entity_type.as_str() {
        "job" => match repo.latest_evidence_by_job_id(entity_id).await {
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
        "release" => match repo.recent_release_attempts(20).await {
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
            match repo.count_selector_misses_since(&since).await {
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

pub(crate) async fn get_ci_bottlenecks(
    project_id: i64,
    ref_name: Option<String>,
    limit: Option<i64>,
    _client: &crate::gitlab_client::GitlabClient,
) -> CapabilityResponse {
    let Ok(repo) = CapabilityRepo::open_default().await else {
        return err("state store unavailable");
    };
    let limit = limit.unwrap_or(20).max(1);
    match repo
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

pub(crate) async fn plan_validation(
    _project_id: i64,
    ref_name: String,
    test_ids: Vec<String>,
) -> CapabilityResponse {
    let Ok(repo) = CapabilityRepo::open_default().await else {
        return err("state store unavailable");
    };
    let since = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::hours(24))
        .unwrap_or(chrono::Utc::now())
        .to_rfc3339();
    let miss_count = repo.count_selector_misses_since(&since).await.unwrap_or(0);

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
