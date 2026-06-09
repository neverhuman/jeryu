use jeryu_core::{CheckConclusion, CheckRun, CheckRunStatus};

use super::*;

pub(crate) fn summarize_checks(checks: &[CheckRun]) -> CheckSummary {
    let queued = checks
        .iter()
        .filter(|check| check.status == CheckRunStatus::Queued)
        .count();
    let running = checks
        .iter()
        .filter(|check| check.status == CheckRunStatus::InProgress)
        .count();
    let failing = checks.iter().filter(|check| failing_check(check)).count();
    let successful = checks
        .iter()
        .filter(|check| {
            check.status == CheckRunStatus::Completed
                && check.conclusion == Some(CheckConclusion::Success)
        })
        .count();
    CheckSummary {
        total: checks.len(),
        queued,
        running,
        failing,
        successful,
        missing: checks.is_empty(),
    }
}

pub(crate) fn check_state(check: &CheckRun) -> EvidenceState {
    match check.status {
        CheckRunStatus::Queued => EvidenceState::Queued,
        CheckRunStatus::InProgress => EvidenceState::Fresh,
        CheckRunStatus::Completed if failing_check(check) => EvidenceState::Failed,
        CheckRunStatus::Completed => EvidenceState::Fresh,
    }
}

pub(crate) fn failing_check(check: &CheckRun) -> bool {
    matches!(
        check.conclusion,
        Some(
            CheckConclusion::ActionRequired
                | CheckConclusion::Cancelled
                | CheckConclusion::Failure
                | CheckConclusion::TimedOut
        )
    )
}

pub(crate) fn check_status(status: &CheckRunStatus) -> &'static str {
    match status {
        CheckRunStatus::Queued => "queued",
        CheckRunStatus::InProgress => "in_progress",
        CheckRunStatus::Completed => "completed",
    }
}

pub(crate) fn check_conclusion(conclusion: &CheckConclusion) -> &'static str {
    match conclusion {
        CheckConclusion::ActionRequired => "action_required",
        CheckConclusion::Cancelled => "cancelled",
        CheckConclusion::Failure => "failure",
        CheckConclusion::Neutral => "neutral",
        CheckConclusion::Success => "success",
        CheckConclusion::Skipped => "skipped",
        CheckConclusion::Superseded => "stale",
        CheckConclusion::TimedOut => "timed_out",
    }
}
