use super::*;

pub async fn build_pipeline_doctor_report(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<PipelineDoctorReport> {
    let root = crate::settings::release_repo_root();
    let schema_result = load_ci_schema(&root).await;
    let schema_context = pipeline_doctor_schema_context_from_result(&schema_result);
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let (runners, runner_inventory_degraded_reason) = match client.list_all_runner_details().await {
        Ok(runners) => (runners, None),
        Err(err) => {
            tracing::warn!(
                target: "jeryu.release.pipeline",
                error = %err,
                "runner inventory unavailable"
            );
            (Vec::new(), Some(err.to_string()))
        }
    };
    let historical_bottlenecks = match Db::open().await {
        Ok(db) => match db
            .ci_job_bottlenecks(project_id, Some(&pipeline.ref_name), 500)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!(
                    target: "jeryu.release.pipeline",
                    error = %err,
                    project_id,
                    pipeline_id,
                    "historical bottleneck query unavailable"
                );
                Vec::new()
            }
        },
        Err(err) => {
            tracing::warn!(
                target: "jeryu.release.pipeline",
                error = %err,
                project_id,
                pipeline_id,
                "historical bottleneck database unavailable"
            );
            Vec::new()
        }
    };
    let schema_pools = match &schema_result {
        Ok(schema) => schema
            .jobs
            .iter()
            .map(|job| (job.id.clone(), job.runner_pool.clone()))
            .collect::<HashMap<_, _>>(),
        Err(err) => {
            tracing::warn!(
                target: "jeryu.release.pipeline",
                error = %err,
                project_id,
                pipeline_id,
                "CI schema unavailable for runner pool mapping"
            );
            HashMap::new()
        }
    };

    let mut doctor_jobs = Vec::new();
    for job in jobs {
        let active = matches!(
            job.status.as_str(),
            "running" | "pending" | "created" | "waiting_for_resource" | "preparing"
        );
        let mut trace_bytes = None;
        let mut trace_tail = None;
        let mut source_fetch_auth_suspected = false;
        if matches!(job.status.as_str(), "running" | "failed")
            && let Ok(trace) = client.job_trace(project_id, job.id).await
        {
            trace_bytes = Some(trace.len());
            trace_tail = Some(
                trace
                    .lines()
                    .rev()
                    .filter(|line| !line.trim().is_empty())
                    .take(5)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            source_fetch_auth_suspected = crate::ci_failure::is_source_fetch_auth_failure(&trace);
        }
        if !active && !source_fetch_auth_suspected {
            continue;
        }
        let canonical_name = canonical_job_name(&job.name);
        let runner_pool = schema_pools
            .get(&canonical_name)
            .cloned()
            .unwrap_or("unknown".to_string());
        let historical = historical_bottlenecks
            .iter()
            .filter(|row| row.job_name == canonical_name)
            .max_by_key(|row| {
                (
                    row.runner_pool.as_deref() == Some(runner_pool.as_str()),
                    row.runs,
                )
            });
        let duration = job.duration.or(job.queued_duration);
        let trace_empty = trace_bytes == Some(0) || trace_tail.as_deref().unwrap_or("").is_empty();
        let historical_avg_duration_secs = historical.map(|row| row.avg_duration_secs);
        let historical_max_duration_secs = historical.and_then(|row| row.max_duration_secs);
        let historical_runs = historical.map(|row| row.runs);
        let slow_factor = historical_avg_duration_secs
            .filter(|avg| *avg > 0.0)
            .and_then(|avg| duration.map(|current| current / avg));
        let queue_factor = historical_avg_duration_secs
            .filter(|avg| *avg > 0.0)
            .and_then(|avg| job.queued_duration.map(|queued| queued / avg));
        let trace_age_suspected = job.status == "running"
            && trace_empty
            && (slow_factor.map(|factor| factor >= 1.5).unwrap_or(false)
                || duration.unwrap_or(0.0) > 900.0);
        let runner_eligibility_issue = runner_eligibility_issue(&job, &runners);
        let stuck_suspected = runner_eligibility_issue.is_some()
            || source_fetch_auth_suspected
            || match job.status.as_str() {
                "running" => {
                    trace_age_suspected
                        || slow_factor
                            .map(|factor| factor >= 2.0)
                            .unwrap_or(duration.unwrap_or(0.0) > 600.0)
                }
                "pending" | "created" | "waiting_for_resource" | "preparing" => queue_factor
                    .map(|factor| factor >= 2.0)
                    .unwrap_or(job.queued_duration.unwrap_or(0.0) > 600.0),
                _ => false,
            };
        let recommendation = if let Some(issue) = &runner_eligibility_issue {
            issue.clone()
        } else if source_fetch_auth_suspected {
            crate::ci_failure::source_fetch_auth_incident_summary().to_string()
        } else if trace_age_suspected {
            let avg = historical_avg_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("n/a".to_string());
            let slow = slow_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or("n/a".to_string());
            format!(
                "trace appears older than historical runtime; inspect trace capture and refresh the runner before running again (avg={avg}, slow={slow})"
            )
        } else if stuck_suspected && job.status == "running" {
            "cancel this job or restart its runner; it is materially slower than historical timing"
                .to_string()
        } else if stuck_suspected {
            "check runner capacity and tags for this pool; queue time is materially above historical timing".to_string()
        } else if job.status == "running" {
            "job is running; compare runtime against historical avg/max and inspect trace if it remains slow".to_string()
        } else {
            "waiting for eligible runner".to_string()
        };
        doctor_jobs.push(PipelineDoctorJob {
            id: job.id,
            name: job.name,
            canonical_name,
            status: job.status,
            stage: job.stage,
            job_tags: job.tag_list,
            runner_pool,
            runner: job.runner.and_then(|runner| runner.description),
            started_at: job.started_at,
            duration_secs: job.duration,
            queued_duration_secs: job.queued_duration,
            historical_avg_duration_secs,
            historical_max_duration_secs,
            historical_runs,
            slow_factor,
            queue_factor,
            trace_bytes,
            trace_tail,
            stuck_suspected,
            trace_age_suspected,
            source_fetch_auth_suspected,
            runner_eligibility_issue,
            recommendation,
        });
    }
    let stuck_suspected = doctor_jobs
        .iter()
        .filter(|job| job.stuck_suspected)
        .cloned()
        .collect::<Vec<_>>();
    Ok(PipelineDoctorReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project_id,
        pipeline_id,
        pipeline_sha: pipeline.sha,
        pipeline_ref: pipeline.ref_name,
        pipeline_status: pipeline.status,
        schema_context,
        runner_inventory_degraded_reason,
        jobs: doctor_jobs,
        stuck_suspected,
    })
}

pub async fn pipeline_doctor_schema_context() -> PipelineDoctorSchemaContext {
    let root = crate::settings::release_repo_root();
    let schema_result = load_ci_schema(&root).await;
    pipeline_doctor_schema_context_from_result(&schema_result)
}

fn pipeline_doctor_schema_context_from_result(
    schema_result: &Result<CiSchema>,
) -> PipelineDoctorSchemaContext {
    match schema_result {
        Ok(schema) => PipelineDoctorSchemaContext {
            available: true,
            source: "veox-testctl ci-schema".to_string(),
            job_count: schema.jobs.len(),
            degraded_reason: None,
        },
        Err(err) => PipelineDoctorSchemaContext {
            available: false,
            source: "veox-testctl ci-schema".to_string(),
            job_count: 0,
            degraded_reason: Some(err.to_string()),
        },
    }
}

fn runner_eligibility_issue(
    job: &crate::gitlab_client::Job,
    runners: &[crate::gitlab_client::RunnerInfo],
) -> Option<String> {
    if !matches!(
        job.status.as_str(),
        "pending" | "created" | "waiting_for_resource" | "preparing"
    ) || !job.tag_list.is_empty()
    {
        return None;
    }

    if runners
        .iter()
        .any(|runner| !runner.paused.unwrap_or(false) && runner.run_untagged)
    {
        return None;
    }

    let paused = runners
        .iter()
        .filter(|runner| runner.paused.unwrap_or(false))
        .count();
    let unpaused = runners.len().saturating_sub(paused);
    let unpaused_tagged = runners
        .iter()
        .filter(|runner| !runner.paused.unwrap_or(false))
        .filter(|runner| !runner.tag_list.is_empty())
        .count();
    let unpaused_run_untagged_false = runners
        .iter()
        .filter(|runner| !runner.paused.unwrap_or(false))
        .filter(|runner| !runner.run_untagged)
        .count();

    Some(format!(
        "pending untagged job has no eligible untagged runner: {unpaused} unpaused runner(s), {unpaused_run_untagged_false} with run_untagged=false, {unpaused_tagged} carrying tags, {paused} paused"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(status: &str, tags: Vec<&str>) -> crate::gitlab_client::Job {
        crate::gitlab_client::Job {
            id: 1,
            name: "unit".into(),
            status: status.into(),
            stage: "test".into(),
            allow_failure: false,
            pipeline_id: Some(1),
            pipeline: None,
            ref_name: Some("main".into()),
            web_url: None,
            queued_duration: None,
            duration: None,
            started_at: None,
            finished_at: None,
            tag_list: tags.into_iter().map(str::to_string).collect(),
            runner: None,
        }
    }

    fn runner(
        paused: bool,
        run_untagged: bool,
        tags: Vec<&str>,
    ) -> crate::gitlab_client::RunnerInfo {
        crate::gitlab_client::RunnerInfo {
            id: 1,
            description: Some("jeryu-default".into()),
            paused: Some(paused),
            tag_list: tags.into_iter().map(str::to_string).collect(),
            run_untagged,
        }
    }

    #[test]
    fn explains_pending_untagged_job_without_untagged_runner() {
        let issue = runner_eligibility_issue(
            &job("pending", vec![]),
            &[runner(false, false, vec!["ci"]), runner(true, true, vec![])],
        )
        .expect("eligibility issue");

        assert!(issue.contains("pending untagged job has no eligible untagged runner"));
        assert!(issue.contains("run_untagged=false"));
    }

    #[test]
    fn tagged_jobs_do_not_report_untagged_runner_issue() {
        let issue = runner_eligibility_issue(
            &job("pending", vec!["ci"]),
            &[runner(false, false, vec!["ci"])],
        );

        assert!(issue.is_none());
    }

    #[test]
    fn missing_ci_schema_is_degraded_context_not_error() {
        let err = anyhow::anyhow!("failed to run veox-testctl ci-schema");
        let context = pipeline_doctor_schema_context_from_result(&Err(err));

        assert!(!context.available);
        assert_eq!(context.source, "veox-testctl ci-schema");
        assert_eq!(context.job_count, 0);
        assert!(context.degraded_reason.is_some());
    }
}
