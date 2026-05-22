use super::*;
use anyhow::Result;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Deserialize)]
pub(crate) struct VtiSkippedArtifact {
    #[serde(default)]
    pub(crate) mode: Option<String>,
    #[serde(default)]
    pub(crate) skipped_jobs: Vec<String>,
    #[serde(default)]
    pub(crate) materialized_jobs: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct VtiGraphMetadata {
    pub(crate) selected_graph: bool,
    pub(crate) materialized_jobs: HashSet<String>,
}

pub(crate) async fn apply_vti_skipped_statuses(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    aggregated: &mut HashMap<String, AggregatedPipelineJob>,
) -> Result<VtiGraphMetadata> {
    let mut metadata = VtiGraphMetadata::default();
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    for job in jobs
        .iter()
        .filter(|job| job.name == "plan-tests" && job.status == "success")
    {
        let Ok(raw) = client
            .job_artifact_file(project_id, job.id, "target/jeryu/vti-skipped.json")
            .await
        else {
            continue;
        };
        let Ok(skipped) = serde_json::from_str::<VtiSkippedArtifact>(&raw) else {
            continue;
        };
        if matches!(skipped.mode.as_deref(), Some("selected" | "docs_only")) {
            metadata.selected_graph = true;
        }
        metadata
            .materialized_jobs
            .extend(skipped.materialized_jobs.into_iter());
        for job_name in skipped.skipped_jobs {
            aggregated
                .entry(job_name)
                .or_insert_with(|| AggregatedPipelineJob {
                    status: "vti-skipped".to_string(),
                    stage: None,
                });
        }
    }
    Ok(metadata)
}

pub(crate) fn apply_vti_selected_omissions(
    schema_jobs: &[CiSchemaJob],
    metadata: &VtiGraphMetadata,
    aggregated: &mut HashMap<String, AggregatedPipelineJob>,
) {
    if !metadata.selected_graph {
        return;
    }
    for job in schema_jobs {
        if metadata.materialized_jobs.contains(&job.id) {
            continue;
        }
        aggregated
            .entry(job.id.clone())
            .or_insert_with(|| AggregatedPipelineJob {
                status: "vti-skipped".to_string(),
                stage: None,
            });
    }
}
