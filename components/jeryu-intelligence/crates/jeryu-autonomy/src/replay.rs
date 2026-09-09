//! Verdict / decision replay.
//!
//! Walks the launch-ledger read-only (recorded_at ASC) to reconstruct the
//! decision timeline for one subject (intent → lease → reviews → verdict →
//! passport → rollback). Counts non-ed25519 signatures so a clean trail can be
//! distinguished from one carrying draft receipts.

use crate::ledger::kind_to_str;
use crate::seam::{LedgerFilter, SeamResult, VerdictLedger};
use crate::types::{LaunchLedgerEntry, LedgerKind};
use chrono::{DateTime, Utc};

/// One event in the reconstructed timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEvent {
    pub recorded_at: DateTime<Utc>,
    pub kind: LedgerKind,
    pub kind_str: &'static str,
    pub actor: String,
    pub signature_algo: String,
}

/// Aggregate counters over the trail.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReplaySummary {
    pub total_events: usize,
    pub verdicts_issued: usize,
    pub escalations: usize,
    pub kill_bell_events: usize,
    /// Number of entries NOT signed with ed25519. A clean trail is 0.
    pub non_ed25519_signature_count: usize,
}

/// Full replay report for one subject.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayReport {
    pub subject_id: String,
    pub timeline: Vec<TimelineEvent>,
    pub summary: ReplaySummary,
}

/// Reconstruct the timeline for a single subject id from the ledger.
pub async fn replay_subject(
    ledger: &dyn VerdictLedger,
    subject_id: &str,
) -> SeamResult<ReplayReport> {
    let entries = ledger
        .list(&LedgerFilter {
            subject_id: Some(subject_id.to_string()),
            ..Default::default()
        })
        .await?;
    Ok(build_report(subject_id, entries))
}

fn build_report(subject_id: &str, mut entries: Vec<LaunchLedgerEntry>) -> ReplayReport {
    // list() already sorts recorded_at ASC, but be defensive.
    entries.sort_by_key(|e| e.recorded_at);
    let mut summary = ReplaySummary::default();
    let timeline: Vec<TimelineEvent> = entries
        .iter()
        .map(|e| {
            summary.total_events += 1;
            match e.kind {
                LedgerKind::VerdictIssued => summary.verdicts_issued += 1,
                LedgerKind::HumanEscalationRequested => summary.escalations += 1,
                LedgerKind::KillBellEngaged | LedgerKind::KillBellResumed => {
                    summary.kill_bell_events += 1
                }
                _ => {}
            }
            if e.signature.algo != "ed25519" {
                summary.non_ed25519_signature_count += 1;
            }
            TimelineEvent {
                recorded_at: e.recorded_at,
                kind: e.kind,
                kind_str: kind_to_str(e.kind),
                actor: e.actor.clone(),
                signature_algo: e.signature.algo.clone(),
            }
        })
        .collect();
    ReplayReport {
        subject_id: subject_id.to_string(),
        timeline,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{MemoryLedger, sign_entry};
    use crate::signing::{EdSigningKey, Signature};
    use crate::types::SchemaTag;
    use chrono::Duration;

    fn entry(
        id: &str,
        subject: &str,
        kind: LedgerKind,
        at: DateTime<Utc>,
        sign: bool,
    ) -> LaunchLedgerEntry {
        let mut e = LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: subject.into(),
            repo: Some("owner/repo".into()),
            payload: serde_json::json!({}),
            recorded_at: at,
            actor: "judge.v1".into(),
            signature: Signature::default_unsigned(),
        };
        if sign {
            sign_entry(&mut e, &EdSigningKey::generate("judge.v1"));
        }
        e
    }

    #[tokio::test]
    async fn replay_reconstructs_ordered_timeline() {
        let ledger = MemoryLedger::new();
        let t0 = Utc::now();
        // Append out of order; replay must order by recorded_at ASC.
        ledger
            .append(&entry(
                "c",
                "vgv_1",
                LedgerKind::MergePassportIssued,
                t0 + Duration::seconds(20),
                true,
            ))
            .await
            .unwrap();
        ledger
            .append(&entry("a", "vgv_1", LedgerKind::ReviewCompleted, t0, true))
            .await
            .unwrap();
        ledger
            .append(&entry(
                "b",
                "vgv_1",
                LedgerKind::VerdictIssued,
                t0 + Duration::seconds(10),
                true,
            ))
            .await
            .unwrap();
        // A different subject must not appear.
        ledger
            .append(&entry(
                "z",
                "vgv_other",
                LedgerKind::VerdictIssued,
                t0,
                true,
            ))
            .await
            .unwrap();

        let report = replay_subject(&ledger, "vgv_1").await.unwrap();
        assert_eq!(report.timeline.len(), 3);
        assert_eq!(
            report.timeline[0].kind_str, "review_completed",
            "earliest first"
        );
        assert_eq!(report.timeline[1].kind_str, "verdict_issued");
        assert_eq!(report.timeline[2].kind_str, "merge_passport_issued");
        assert_eq!(report.summary.total_events, 3);
        assert_eq!(report.summary.verdicts_issued, 1);
        assert_eq!(
            report.summary.non_ed25519_signature_count, 0,
            "a fully-signed trail must report zero non-ed25519 signatures"
        );
    }

    #[tokio::test]
    async fn replay_counts_kill_bell_and_escalation_events() {
        let ledger = MemoryLedger::new();
        let t0 = Utc::now();
        ledger
            .append(&entry(
                "a",
                "subj",
                LedgerKind::HumanEscalationRequested,
                t0,
                true,
            ))
            .await
            .unwrap();
        ledger
            .append(&entry(
                "b",
                "subj",
                LedgerKind::KillBellEngaged,
                t0 + Duration::seconds(1),
                true,
            ))
            .await
            .unwrap();
        ledger
            .append(&entry(
                "c",
                "subj",
                LedgerKind::KillBellResumed,
                t0 + Duration::seconds(2),
                true,
            ))
            .await
            .unwrap();
        let report = replay_subject(&ledger, "subj").await.unwrap();
        assert_eq!(report.summary.escalations, 1);
        assert_eq!(report.summary.kill_bell_events, 2);
    }

    #[tokio::test]
    async fn replay_empty_subject_yields_empty_report() {
        let ledger = MemoryLedger::new();
        let report = replay_subject(&ledger, "nope").await.unwrap();
        assert_eq!(report.summary, ReplaySummary::default());
        assert!(report.timeline.is_empty());
    }
}
