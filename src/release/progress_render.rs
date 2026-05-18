use super::*;

pub(crate) fn effective_job_status<'a>(
    state: Option<&'a AggregatedPipelineJob>,
    pipeline_status: &str,
) -> &'a str {
    match state {
        Some(state) => state.status.as_str(),
        None => match pipeline_status {
            "success" | "failed" | "canceled" | "skipped" => "omitted",
            _ => "pending",
        },
    }
}

pub(crate) fn pipeline_item(
    job: &CiSchemaJob,
    state: Option<&AggregatedPipelineJob>,
    effective_status: &str,
) -> PipelineExplainItem {
    PipelineExplainItem {
        id: job.id.clone(),
        status: display_job_status(effective_status).to_string(),
        stage: state.and_then(|s| s.stage.clone()),
        runner_pool: job.runner_pool.clone(),
        kind: job.kind.clone(),
        component: job.component.clone(),
        evidence_driven: job.evidence_driven,
        estimated_cost: job.estimated_cost.clone(),
        evidence_outputs: job.evidence_outputs.clone(),
        depends_on: job.depends_on.clone(),
    }
}

pub(crate) fn display_job_status(status: &str) -> &str {
    match status {
        "omitted" => "vti-skipped",
        other => other,
    }
}

pub(crate) fn write_pipeline_item_section(
    out: &mut String,
    heading: &str,
    items: &[PipelineExplainItem],
) {
    if items.is_empty() {
        return;
    }

    use std::fmt::Write as _;

    let _ = writeln!(out);
    let _ = writeln!(out, "  {heading}:");
    for item in items {
        let _ = writeln!(
            out,
            "    - {} [{} / {} / {}]",
            item.id,
            item.runner_pool,
            item.kind,
            display_job_status(&item.status)
        );
    }
}

pub(crate) trait LaneProgressSummaryView {
    fn release_critical_progress(&self) -> &LaneProgress;
    fn extended_progress(&self) -> &LaneProgress;
    fn research_progress(&self) -> &LaneProgress;
}

macro_rules! impl_lane_progress_summary_view {
    ($ty:ty) => {
        impl LaneProgressSummaryView for $ty {
            fn release_critical_progress(&self) -> &LaneProgress {
                &self.release_critical
            }

            fn extended_progress(&self) -> &LaneProgress {
                &self.extended
            }

            fn research_progress(&self) -> &LaneProgress {
                &self.research
            }
        }
    };
}

impl_lane_progress_summary_view!(ProgressReport);
impl_lane_progress_summary_view!(PipelineExplainReport);

pub(crate) fn write_lane_progress_summary<T: LaneProgressSummaryView>(
    out: &mut String,
    report: &T,
    indent: &str,
    release_critical_label: &str,
) {
    use std::fmt::Write as _;

    let _ = writeln!(
        out,
        "{indent}{release_critical_label}: {}/{} ({:.1}%)",
        report.release_critical_progress().passed,
        report.release_critical_progress().total,
        report.release_critical_progress().percent
    );
    let _ = writeln!(
        out,
        "{indent}Extended:         {}/{} ({:.1}%)",
        report.extended_progress().passed,
        report.extended_progress().total,
        report.extended_progress().percent
    );
    let _ = writeln!(
        out,
        "{indent}Research:         {}/{} ({:.1}%)",
        report.research_progress().passed,
        report.research_progress().total,
        report.research_progress().percent
    );
}

pub fn render_progress_text(report: &ProgressReport) -> String {
    let mut out = String::new();
    use std::fmt::Write as _;

    let _ = writeln!(out, "━━━ jeryu progress ━━━");
    let _ = writeln!(out, "  Generated:         {}", report.generated_at);
    let _ = writeln!(out, "  Ref:               {}", report.ref_name);
    let _ = writeln!(
        out,
        "  Latest pipeline:   {:?} status={} sha={}",
        report.latest_pipeline_id,
        report.latest_pipeline_status.as_deref().unwrap_or("(none)"),
        report.latest_pipeline_sha.as_deref().unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Winning pipeline:  {:?} sha={} version={}",
        report.winning_pipeline_id,
        report.winning_sha.as_deref().unwrap_or("(none)"),
        report
            .expected_release_version
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(out);
    write_lane_progress_summary(&mut out, report, "  ", "Release-Critical");
    let _ = writeln!(
        out,
        "  Release Execution: {:.1}% freshness={} phase={}",
        report.release_execution.percent,
        report.punchlist_freshness,
        report
            .release_execution
            .phase
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Latest attempt:    sha={} state={}",
        report
            .release_execution
            .latest_attempt_sha
            .as_deref()
            .unwrap_or("(none)"),
        report
            .release_execution
            .latest_attempt_state
            .as_deref()
            .unwrap_or("(none)")
    );
    let _ = writeln!(
        out,
        "  Current blocker:   {}",
        report.current_blocker.as_deref().unwrap_or("(none)")
    );
    if !report.blocking_remaining.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Blocking remaining:");
        for job in &report.blocking_remaining {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    if !report.non_blocking_failed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "  Non-blocking failed:");
        for job in &report.non_blocking_failed {
            let _ = writeln!(out, "    - {}", job);
        }
    }
    out
}
