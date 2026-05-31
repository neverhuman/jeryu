//! Pure bug-domain logic shared by every backend.
//!
//! Ported from jeryu's `bugtracker/ops.rs`. These functions have no I/O and no
//! store dependency: stable id minting, branch slugging, the terminal-status
//! transition guard, the triage ranking key, and report JSON parsing. The store
//! seam reuses `ranking_key`/`sort_bugs` so in-memory and durable backends agree.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::enums::{BugSort, BugStatus};
use super::records::BugRecord;
use super::types::CanonicalBugReport;

/// Deterministic `bug-<10hex>` id from target/source/title plus the submit timestamp.
pub fn generate_bug_id(report: &CanonicalBugReport, now: DateTime<Utc>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(report.target_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.source_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.title.as_bytes());
    hasher.update(b"\0");
    // `timestamp_nanos_opt` only returns None when the submit timestamp falls
    // outside the range representable as nanoseconds since the epoch
    // (year < 1677 or > 2262). Use an explicit 0 sentinel so id generation
    // stays deterministic for such clocks, which cannot occur in practice.
    let submit_nanos = now.timestamp_nanos_opt().unwrap_or(0);
    hasher.update(submit_nanos.to_string());
    let digest = hasher.finalize();
    format!("bug-{}", hex::encode(&digest[..5]))
}

/// Working-branch name for a bug fix: `bug/<id>-<slug>` (slug capped at 6 words).
pub fn branch_name(bug_id: &str, title: &str) -> String {
    let slug = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "bug/{bug_id}-{}",
        if slug.is_empty() { "fix" } else { &slug }
    )
}

/// Reject reopening a terminal bug; any other transition (including same-status) is allowed.
pub fn validate_transition(from: BugStatus, to: BugStatus) -> Result<()> {
    if from.is_terminal() && from != to {
        bail!(
            "terminal bug status {} cannot transition to {}",
            from.as_str(),
            to.as_str()
        );
    }
    Ok(())
}

/// Triage ranking key: severity, then priority, then readiness, difficulty,
/// fewer failed attempts first, then most-recently-updated.
pub fn ranking_key(bug: &BugRecord) -> (u8, u8, u8, u8, i64, String) {
    let ready_rank = match bug.status {
        BugStatus::Ready => 0,
        BugStatus::Accepted | BugStatus::NeedsTriage => 1,
        BugStatus::Blocked | BugStatus::NeedsInfo => 3,
        status if status.is_terminal() => 5,
        _ => 2,
    };
    (
        bug.severity as u8,
        bug.priority as u8,
        ready_rank,
        bug.difficulty,
        -bug.failed_attempt_count,
        bug.updated_at.clone(),
    )
}

/// Sort a slice of bugs in place according to the requested order.
///
/// Shared by the store seam so every backend produces the same ordering.
pub fn sort_bugs(bugs: &mut [BugRecord], sort: BugSort) {
    match sort {
        BugSort::Rank => bugs.sort_by_key(ranking_key),
        BugSort::Severity => bugs.sort_by_key(|bug| bug.severity),
        BugSort::Priority => bugs.sort_by_key(|bug| bug.priority),
        BugSort::Difficulty => bugs.sort_by_key(|bug| bug.difficulty),
        BugSort::Ready => {
            bugs.sort_by_key(|bug| if bug.status == BugStatus::Ready { 0 } else { 1 })
        }
        BugSort::Updated => bugs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
        BugSort::Attempts => bugs.sort_by_key(|bug| -bug.attempt_count),
    }
}

/// Parse a canonical bug report from JSON.
pub fn parse_report_json(input: &str) -> Result<CanonicalBugReport> {
    serde_json::from_str(input).context("parse canonical bug report JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::enums::{BugPriority, BugSeverity};

    fn report() -> CanonicalBugReport {
        CanonicalBugReport {
            target_project: "redlinedb".into(),
            source_project: "veox".into(),
            title: "redline adapter loses writes".into(),
            component: Some("adapter".into()),
            current_behavior: "writes disappear".into(),
            expected_behavior: "writes persist".into(),
            environment: "local".into(),
            frequency: "always".into(),
            impact: "blocks local agent state".into(),
            security_privacy: "no security impact".into(),
            no_secrets_confirmed: true,
            reproduction_steps: vec!["submit write".into(), "restart".into()],
            evidence: Vec::new(),
            acceptance_criteria: vec!["write survives restart".into()],
            severity: BugSeverity::S1,
            priority: BugPriority::P1,
            difficulty: 3,
        }
    }

    #[test]
    fn validation_requires_no_secrets_confirmation() {
        let mut r = report();
        r.no_secrets_confirmed = false;
        assert!(r.validate().is_err());
    }

    #[test]
    fn validation_lands_missing_repro_in_needs_info() {
        let mut r = report();
        r.reproduction_steps.clear();
        assert_eq!(r.validate().unwrap(), BugStatus::NeedsInfo);
    }

    #[test]
    fn generated_ids_use_bug_prefix_and_hash_length() {
        let id = generate_bug_id(&report(), Utc::now());
        assert!(id.starts_with("bug-"));
        assert_eq!(id.len(), 14);
    }

    #[test]
    fn terminal_status_blocks_reopen() {
        assert!(validate_transition(BugStatus::Done, BugStatus::Ready).is_err());
        assert!(validate_transition(BugStatus::Ready, BugStatus::Done).is_ok());
    }

    #[test]
    fn branch_name_slugs_title() {
        let b = branch_name("bug-0123456789", "Adapter loses Writes!!");
        assert_eq!(b, "bug/bug-0123456789-adapter-loses-writes");
    }
}
