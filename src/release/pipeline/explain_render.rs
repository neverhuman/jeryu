use std::fmt::Write as _;

use super::*;

pub fn render_pipeline_explain_text(report: &PipelineExplainReport) -> String {
    let mut out = String::new();
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
