use super::*;
use anyhow::Result;
use tracing::warn;

pub(crate) async fn production_promotion_pipeline_id(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: &str,
) -> Result<Option<i64>> {
    for pipeline in client
        .list_pipelines(project_id, Some(ref_name))
        .await?
        .into_iter()
    {
        if !pipeline_matches_release_sha(client, project_id, pipeline.id, &pipeline.sha, sha)
            .await?
        {
            continue;
        }
        let jobs = aggregate_pipeline_jobs(
            client
                .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
                .await?,
        );
        let Some(job) = jobs.get("promote-production-final") else {
            continue;
        };
        if matches!(
            job.status.as_str(),
            "created" | "pending" | "running" | "success"
        ) {
            return Ok(Some(pipeline.id));
        }
    }
    Ok(None)
}

async fn pipeline_matches_release_sha(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    pipeline_sha: &str,
    release_sha: &str,
) -> Result<bool> {
    if pipeline_sha == release_sha {
        return Ok(true);
    }
    match client
        .list_pipeline_variables(project_id, pipeline_id)
        .await
    {
        Ok(variables) => Ok(variables.iter().any(|variable| {
            matches!(variable.key.as_str(), "JERYU_RELEASE_SHA") && variable.value == release_sha
        })),
        Err(err) => {
            warn!(
                project_id,
                pipeline_id,
                error = %err,
                "could not inspect pipeline variables while checking production promotion"
            );
            Ok(false)
        }
    }
}
