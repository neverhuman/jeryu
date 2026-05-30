use super::*;

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
    if report.schema_context.available {
        let _ = writeln!(
            out,
            "  Schema:   available ({} job definitions)",
            report.schema_context.job_count
        );
    } else {
        let _ = writeln!(
            out,
            "  Schema:   degraded ({})",
            report.schema_context.source
        );
        if let Some(reason) = &report.schema_context.degraded_reason {
            let _ = writeln!(out, "            {reason}");
        }
    }
    if let Some(reason) = &report.runner_inventory_degraded_reason {
        let _ = writeln!(out, "  Runners:  degraded");
        let _ = writeln!(out, "            {reason}");
    }
    let _ = writeln!(out, "  Jobs:     {}", report.jobs.len());
    let _ = writeln!(out, "  Suspect:  {}", report.stuck_suspected.len());
    if !report.jobs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Active/source-health jobs:");
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
            if job.source_fetch_auth_suspected {
                let _ = writeln!(out, "      source-fetch: auth failed before user code ran");
            }
            if let Some(issue) = &job.runner_eligibility_issue {
                let _ = writeln!(out, "      runner-eligibility: {issue}");
            }
        }
    }
    out
}
