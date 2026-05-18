use super::*;

pub(crate) fn canonical_job_name(name: &str) -> String {
    let Some((prefix, suffix)) = name.rsplit_once(' ') else {
        return name.to_string();
    };
    let Some((lhs, rhs)) = suffix.split_once('/') else {
        return name.to_string();
    };
    if lhs.parse::<usize>().is_ok() && rhs.parse::<usize>().is_ok() {
        prefix.to_string()
    } else {
        name.to_string()
    }
}

pub(crate) fn status_rank(status: &str) -> usize {
    match status {
        "failed" | "canceled" => 6,
        "running" => 5,
        "pending" | "created" | "waiting_for_resource" | "preparing" => 4,
        "manual" => 3,
        "success" => 2,
        "skipped" | "vti-skipped" => 1,
        _ => 0,
    }
}

pub(crate) fn merge_status(current: &str, incoming: &str) -> String {
    if status_rank(incoming) >= status_rank(current) {
        incoming.to_string()
    } else {
        current.to_string()
    }
}

pub(crate) fn aggregate_pipeline_jobs(
    jobs: Vec<crate::gitlab_client::Job>,
) -> HashMap<String, AggregatedPipelineJob> {
    let mut aggregated = HashMap::new();
    for job in jobs {
        let key = canonical_job_name(&job.name);
        aggregated
            .entry(key)
            .and_modify(|current: &mut AggregatedPipelineJob| {
                current.status = merge_status(&current.status, &job.status);
                if current.stage.is_none() {
                    current.stage = Some(job.stage.clone());
                }
            })
            .or_insert_with(|| AggregatedPipelineJob {
                status: job.status,
                stage: Some(job.stage),
            });
    }
    aggregated
}

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

/// Fetch, aggregate, and VTI-normalize job statuses for a single pipeline,
/// returning a `HashMap<job_id, status>`. Returns an empty map when `pipeline`
/// is `None`.
pub(crate) async fn collect_pipeline_statuses(
    client: &GitlabClient,
    project_id: i64,
    schema_jobs: &[CiSchemaJob],
    pipeline: Option<&Pipeline>,
) -> Result<HashMap<String, String>> {
    let Some(pipeline) = pipeline else {
        return Ok(HashMap::new());
    };
    let mut aggregated = aggregate_pipeline_jobs(
        client
            .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
            .await?,
    );
    let vti_metadata =
        apply_vti_skipped_statuses(client, project_id, pipeline.id, &mut aggregated).await?;
    apply_vti_selected_omissions(schema_jobs, &vti_metadata, &mut aggregated);
    Ok(aggregated
        .into_iter()
        .map(|(name, state)| (name, state.status))
        .collect())
}

pub(crate) async fn latest_pipeline_for_ref(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<Option<Pipeline>> {
    Ok(client
        .list_pipelines(project_id, Some(ref_name))
        .await?
        .into_iter()
        .next())
}

pub(crate) fn is_release_candidate_job(name: &str) -> bool {
    matches!(
        name,
        "build-enclave-server"
            | "test-live-public-surface"
            | "test-local-built"
            | "publish-rc-dry-run"
            | "test-local-rc"
    )
}

pub(crate) fn jobs_materialize_release_candidate(jobs: &[Job]) -> bool {
    // At least one RC job must have actually succeeded — a skipped or absent RC job
    // means VTI did not match the release surface for this diff.
    jobs.iter()
        .any(|job| is_release_candidate_job(&job.name) && job.status == "success")
}

pub(crate) fn failed_release_candidate_jobs(jobs: &[Job]) -> Vec<String> {
    jobs.iter()
        .filter(|job| is_release_candidate_job(&canonical_job_name(&job.name)))
        .filter(|job| !job.allow_failure)
        .filter(|job| !matches!(job.status.as_str(), "success" | "skipped"))
        .map(|job| job.name.clone())
        .collect()
}

pub(crate) fn aggregated_materializes_release_candidate(
    jobs: &HashMap<String, AggregatedPipelineJob>,
) -> bool {
    jobs.keys().any(|name| is_release_candidate_job(name))
}

pub(crate) async fn latest_release_candidate_pipeline_for_ref(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
) -> Result<Option<Pipeline>> {
    for pipeline in client.list_pipelines(project_id, Some(ref_name)).await? {
        if pipeline.source.as_deref() == Some("parent_pipeline") {
            continue;
        }
        let jobs = client
            .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
            .await?;
        if jobs.iter().any(|job| {
            matches!(
                job.name.as_str(),
                "deploy-canary-final" | "report-testing-punchlist" | "promote-production-final"
            )
        }) {
            continue;
        }
        if pipeline.status != "success" {
            // If all RC jobs were skipped, this is a VTI-only pipeline whose failure
            // (e.g., a non-RC compile job) should not block the release candidate search.
            let rc_jobs: Vec<&Job> = jobs
                .iter()
                .filter(|j| is_release_candidate_job(&j.name))
                .collect();
            let all_rc_skipped =
                !rc_jobs.is_empty() && rc_jobs.iter().all(|j| j.status == "skipped");
            if all_rc_skipped {
                info!(
                    project_id,
                    pipeline_id = pipeline.id,
                    ref_name = %ref_name,
                    status = %pipeline.status,
                    sha = %pipeline.sha,
                    "failed pipeline has all RC jobs skipped (VTI-only); continuing to older pipeline"
                );
                continue;
            }
            info!(
                project_id,
                pipeline_id = pipeline.id,
                ref_name = %ref_name,
                status = %pipeline.status,
                sha = %pipeline.sha,
                "newer ref pipeline is not green; no release candidate is ready"
            );
            return Ok(None);
        }
        let failed_release_jobs = failed_release_candidate_jobs(&jobs);
        if !failed_release_jobs.is_empty() {
            info!(
                project_id,
                pipeline_id = pipeline.id,
                ref_name = %ref_name,
                status = %pipeline.status,
                sha = %pipeline.sha,
                failed_jobs = %failed_release_jobs.join(", "),
                "latest green ref pipeline has failed release-candidate jobs; no release candidate is ready"
            );
            return Ok(None);
        }
        if jobs_materialize_release_candidate(&jobs) {
            return Ok(Some(pipeline));
        }
        // Green pipeline but VTI selected a narrow diff — no RC jobs actually ran.
        // Continue to older pipelines rather than blocking.
        info!(
            project_id,
            pipeline_id = pipeline.id,
            ref_name = %ref_name,
            status = %pipeline.status,
            sha = %pipeline.sha,
            "green pipeline did not materialize RC artifacts (VTI narrow); continuing to older pipeline"
        );
    }
    Ok(None)
}
