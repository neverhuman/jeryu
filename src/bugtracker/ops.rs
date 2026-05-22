use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use super::types::{BugRecord, BugStatus, CanonicalBugReport};

pub fn generate_bug_id(report: &CanonicalBugReport, now: DateTime<Utc>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(report.target_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.source_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.title.as_bytes());
    hasher.update(b"\0");
    hasher.update(now.timestamp_nanos_opt().unwrap_or_default().to_string());
    let digest = hasher.finalize();
    format!("bug-{}", hex::encode(&digest[..5]))
}

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

pub fn parse_report_json(input: &str) -> Result<CanonicalBugReport> {
    serde_json::from_str(input).context("parse canonical bug report JSON")
}

#[cfg(test)]
mod tests {
    use super::super::types::{BugPriority, BugSeverity};
    use super::*;

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
}
