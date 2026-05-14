use super::*;

pub async fn build_pipeline_doctor_report(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
) -> Result<PipelineDoctorReport> {
    let root = crate::settings::release_repo_root();
    let schema = load_ci_schema(&root).await?;
    let pipeline = client.get_pipeline(project_id, pipeline_id).await?;
    let jobs = client
        .list_pipeline_jobs_with_downstream(project_id, pipeline_id)
        .await?;
    let historical_bottlenecks = match Db::open().await {
        Ok(db) => match db
            .ci_job_bottlenecks(project_id, Some(&pipeline.ref_name), 500)
            .await
        {
            Ok(rows) => rows,
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    let schema_pools = schema
        .jobs
        .iter()
        .map(|job| (job.id.clone(), job.runner_pool.clone()))
        .collect::<HashMap<_, _>>();

    let mut doctor_jobs = Vec::new();
    for job in jobs {
        if !matches!(
            job.status.as_str(),
            "running" | "pending" | "created" | "waiting_for_resource" | "preparing"
        ) {
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
        let mut trace_bytes = None;
        let mut trace_tail = None;
        if job.status == "running"
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
        }
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
        let stuck_suspected = match job.status.as_str() {
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
        let recommendation = if trace_age_suspected {
            let avg = historical_avg_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("n/a".to_string());
            let slow = slow_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or("n/a".to_string());
            "trace appears older than historical runtime; inspect trace capture and refresh the runner before running again"
                .to_string()
                + &format!(" (avg={}, slow={})", avg, slow)
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
        jobs: doctor_jobs,
        stuck_suspected,
    })
}

pub fn render_pipeline_doctor_text(report: &PipelineDoctorReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;
    let _ = writeln!(out, "━━━ jeryu pipeline doctor ━━━");
    let _ = writeln!(out, "  Pipeline: {}", report.pipeline_id);
    let _ = writeln!(
        out,
        "  Ref/SHA:  {} / {}",
        report.pipeline_ref, report.pipeline_sha
    );
    let _ = writeln!(out, "  Status:   {}", report.pipeline_status);
    let _ = writeln!(out, "  Active:   {}", report.jobs.len());
    let _ = writeln!(out, "  Suspect:  {}", report.stuck_suspected.len());
    if !report.jobs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Active jobs:");
        for job in &report.jobs {
            let trace = job
                .trace_bytes
                .map(|bytes| format!("{bytes}b trace"))
                .unwrap_or("trace n/a".to_string());
            let current = job
                .duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("-".to_string());
            let queue = job
                .queued_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("-".to_string());
            let avg = job
                .historical_avg_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("-".to_string());
            let max = job
                .historical_max_duration_secs
                .map(|value| format!("{value:.1}s"))
                .unwrap_or("-".to_string());
            let slow = job
                .slow_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or("-".to_string());
            let queue_factor = job
                .queue_factor
                .map(|value| format!("{value:.2}x"))
                .unwrap_or("-".to_string());
            let marker = if job.stuck_suspected { "!" } else { "-" };
            let _ = writeln!(
                out,
                "    {} {} #{} [{} / {} / {}] run={} avg={} max={} slow={} queue={} qslow={} trace={}",
                marker,
                job.canonical_name,
                job.id,
                job.runner_pool,
                job.stage,
                job.status,
                current,
                avg,
                max,
                slow,
                queue,
                queue_factor,
                trace
            );
            if job.stuck_suspected {
                if let Some(runs) = job.historical_runs {
                    let _ = writeln!(out, "      history: {} runs", runs);
                }
                let _ = writeln!(out, "      recommendation: {}", job.recommendation);
            }
            if job.trace_age_suspected {
                let _ = writeln!(out, "      trace: outdated compared with historical timing");
            }
        }
    }
    out
}
