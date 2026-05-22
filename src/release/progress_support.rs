use super::*;

pub(super) fn summary_lane_or_default(
    summary: Option<&serde_json::Value>,
    key: &str,
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, String>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    match summary.and_then(|summary| summary_lane_progress(summary, key)) {
        Some(progress) => progress,
        None => lane_progress(schema, statuses, lane, pipeline_status),
    }
}

pub(super) fn pipeline_lane_progress(
    schema: &[CiSchemaJob],
    statuses: &HashMap<String, AggregatedPipelineJob>,
    lane: &str,
    pipeline_status: &str,
) -> LaneProgress {
    let mut total = 0usize;
    let mut passed = 0usize;
    for job in schema.iter().filter(|job| job.lane == lane) {
        let status = effective_job_status(statuses.get(&job.id), pipeline_status);
        if matches!(status, "omitted" | "skipped" | "vti-skipped") {
            continue;
        }
        total += 1;
        if status == "success" {
            passed += 1;
        }
    }
    let percent = lane_progress_percent(total, passed);
    LaneProgress {
        passed,
        total,
        percent,
    }
}

pub(super) fn lane_progress_percent(total: usize, passed: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (passed as f64 / total as f64) * 100.0
    }
}

pub(super) fn collect_job_ids<F>(jobs: &[CiSchemaJob], predicate: F) -> Vec<String>
where
    F: Fn(&CiSchemaJob) -> bool,
{
    jobs.iter()
        .filter(|job| predicate(job))
        .map(|job| job.id.clone())
        .collect::<Vec<_>>()
}
