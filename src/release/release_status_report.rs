use super::*;
use anyhow::Result;

pub async fn build_release_status_report(
    db: &Db,
    query: ReleaseStatusQuery,
) -> Result<ReleaseStatusReport> {
    let recent = if let Some(sha) = &query.sha {
        let mut attempts = Vec::new();
        if let Some(project_id) = query.project_id {
            if let Some(attempt) = db
                .get_release_attempt(project_id, query.ref_name.as_deref().unwrap_or("main"), sha)
                .await?
            {
                attempts.push(attempt);
            }
        } else {
            attempts = db
                .recent_release_attempts(None, query.ref_name.as_deref(), query.limit as i64)
                .await?;
            attempts.retain(|attempt| attempt.sha == *sha);
        }
        attempts
    } else {
        db.recent_release_attempts(
            query.project_id,
            query.ref_name.as_deref(),
            query.limit as i64,
        )
        .await?
    };

    let latest = recent.first().cloned().map(view_attempt).transpose()?;
    let recent = recent
        .into_iter()
        .map(view_attempt)
        .collect::<Result<Vec<_>>>()?;
    Ok(ReleaseStatusReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id: query.project_id,
        ref_name: query.ref_name,
        sha: query.sha,
        limit: query.limit,
        total_attempts: recent.len(),
        latest,
        recent,
    })
}
