use super::*;

fn explain_release_candidate_blocker(
    aggregated: &HashMap<String, AggregatedPipelineJob>,
    release_candidate_materialized: bool,
    blocking_failed: &[PipelineExplainItem],
    incomplete_milestones: &[PipelineExplainMilestone],
) -> Option<String> {
    if !release_candidate_materialized {
        return Some(if aggregated.is_empty() {
            "materialized pipeline is empty".to_string()
        } else {
            "release candidate jobs omitted by VTI".to_string()
        });
    }
    if let Some(item) = blocking_failed.first() {
        return Some(format!("{} failed on {}", item.id, item.runner_pool));
    }
    incomplete_milestones.first().map(|milestone| {
        format!(
            "{} pending: {}",
            milestone.title,
            milestone.incomplete_jobs.join(", ")
        )
    })
}

pub async fn build_pipeline_explain_report(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<PipelineExplainReport> {
    let root = crate::settings::release_repo_root();
    let schema = load_ci_schema(&root).await?;
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let mut aggregated = aggregate_pipeline_jobs(
        client
            .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
            .await?,
    );
    let vti_metadata =
        apply_vti_skipped_statuses(client, project_id, pipeline_id, &mut aggregated).await?;
    let pipeline_product = if aggregated.contains_key("promote-production-final") {
        "production-promotion"
    } else if aggregated.contains_key("deploy-canary-final")
        || aggregated.contains_key("report-testing-punchlist")
    {
        "release-execution"
    } else {
        "main-candidate"
    };
    let tracked_ids = schema
        .jobs
        .iter()
        .map(|job| job.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let untracked_jobs = aggregated
        .keys()
        .filter(|name| !tracked_ids.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let relevant_jobs = schema
        .jobs
        .iter()
        .filter(|job| match pipeline_product {
            "release-execution" => job.pipeline_product == "release-execution",
            "production-promotion" => job.pipeline_product == "production-promotion",
            _ => {
                job.pipeline_product != "release-execution"
                    && job.pipeline_product != "production-promotion"
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    apply_vti_selected_omissions(&relevant_jobs, &vti_metadata, &mut aggregated);

    let release_critical = pipeline_lane_progress(
        &relevant_jobs,
        &aggregated,
        "release-blocking",
        &pipeline.status,
    );
    let extended =
        pipeline_lane_progress(&relevant_jobs, &aggregated, "extended", &pipeline.status);
    let research =
        pipeline_lane_progress(&relevant_jobs, &aggregated, "research", &pipeline.status);
    let release_execution = pipeline_lane_progress(
        &relevant_jobs,
        &aggregated,
        "release-execution",
        &pipeline.status,
    );

    let mut blocking_failed = Vec::new();
    let mut blocking_pending = Vec::new();
    let mut non_blocking_failed = Vec::new();
    let mut non_blocking_pending = Vec::new();
    for job in &relevant_jobs {
        let state = aggregated.get(&job.id);
        let status = effective_job_status(state, &pipeline.status);
        if matches!(status, "success" | "skipped" | "omitted" | "vti-skipped") {
            continue;
        }
        let item = pipeline_item(job, state, status);
        if matches!(status, "failed" | "canceled") {
            if job.release_blocking {
                blocking_failed.push(item);
            } else {
                non_blocking_failed.push(item);
            }
        } else if job.release_blocking {
            blocking_pending.push(item);
        } else {
            non_blocking_pending.push(item);
        }
    }

    let mut incomplete_milestones = Vec::new();
    for milestone in &schema.milestones {
        if milestone.pipeline_product != pipeline_product {
            continue;
        }
        let mut statuses = Vec::new();
        let mut failed = false;
        let mut incomplete = Vec::new();
        for job in &milestone.jobs {
            let status = effective_job_status(aggregated.get(job), &pipeline.status);
            statuses.push(status.to_string());
            if !matches!(status, "success" | "skipped" | "omitted" | "vti-skipped") {
                incomplete.push(job.clone());
            }
            if matches!(status, "failed" | "canceled") {
                failed = true;
            }
        }
        if incomplete.is_empty() {
            continue;
        }
        let status = if failed { "failed" } else { "pending" };
        incomplete_milestones.push(PipelineExplainMilestone {
            id: milestone.id.clone(),
            title: milestone.title.clone(),
            status: status.to_string(),
            lane: milestone.lane.clone(),
            jobs: milestone.jobs.clone(),
            incomplete_jobs: incomplete,
        });
    }

    let release_candidate_materialized = pipeline_product != "main-candidate"
        || aggregated_materializes_release_candidate(&aggregated);

    let current_blocker = explain_release_candidate_blocker(
        &aggregated,
        release_candidate_materialized,
        &blocking_failed,
        &incomplete_milestones,
    );
    let release_eligible =
        release_candidate_materialized && blocking_failed.is_empty() && blocking_pending.is_empty();

    Ok(PipelineExplainReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        pipeline_id,
        pipeline_sha: pipeline.sha,
        pipeline_ref: pipeline.ref_name,
        pipeline_status: pipeline.status,
        release_critical,
        extended,
        research,
        release_execution,
        current_blocker,
        release_eligible,
        blocking_failed,
        blocking_pending,
        non_blocking_failed,
        non_blocking_pending,
        incomplete_milestones,
        untracked_jobs,
    })
}

pub fn render_pipeline_explain_text(report: &PipelineExplainReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "━━━ jeryu pipeline explain ━━━");
    let _ = writeln!(out, "  Pipeline:          {}", report.pipeline_id);
    let _ = writeln!(
        out,
        "  Ref/SHA:           {} / {}",
        report.pipeline_ref, report.pipeline_sha
    );
    let _ = writeln!(out, "  Status:            {}", report.pipeline_status);
    let _ = writeln!(out, "  Release eligible:  {}", report.release_eligible);
    let _ = writeln!(
        out,
        "  Current blocker:   {}",
        report.current_blocker.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "  Lane progress:");
    write_lane_progress_summary(&mut out, report, "    ", "Release-critical");
    if report.release_execution.total > 0 {
        let _ = writeln!(
            out,
            "    Release execution: {}/{} ({:.1}%)",
            report.release_execution.passed,
            report.release_execution.total,
            report.release_execution.percent
        );
    }
    write_pipeline_item_section(&mut out, "Blocking failed", &report.blocking_failed);
    write_pipeline_item_section(&mut out, "Blocking pending", &report.blocking_pending);
    write_pipeline_item_section(&mut out, "Non-blocking failed", &report.non_blocking_failed);
    if !report.incomplete_milestones.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Incomplete milestones:");
        for milestone in &report.incomplete_milestones {
            let _ = writeln!(
                out,
                "    - {} [{}] :: {}",
                milestone.title,
                milestone.status,
                milestone.incomplete_jobs.join(", ")
            );
        }
    }
    if !report.untracked_jobs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Untracked pipeline jobs:");
        for job in &report.untracked_jobs {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pipeline_reports_its_own_blocker() {
        let blocker = explain_release_candidate_blocker(&HashMap::new(), false, &[], &[]);
        assert_eq!(blocker.as_deref(), Some("materialized pipeline is empty"));
    }

    #[test]
    fn omitted_release_candidate_jobs_keep_the_vti_blocker() {
        let aggregated = HashMap::from([(
            "compile-workspace".to_string(),
            AggregatedPipelineJob::default(),
        )]);
        let blocker = explain_release_candidate_blocker(&aggregated, false, &[], &[]);
        assert_eq!(
            blocker.as_deref(),
            Some("release candidate jobs omitted by VTI")
        );
    }
}
