//! Owner: Evidence Gate / autonomy control plane
//! Proof: `cargo test -p jeryu -- autonomy::replay`
//! Invariants:
//!   - The replay walks the launch_ledger read-only; it never mutates rows.
//!   - The timeline is ordered by `recorded_at ASC` so the resulting walk
//!     matches wall-clock ordering of the underlying decisions.
//!   - `payload_excerpt` is intentionally shallow (first-level scalar fields
//!     only) so the audit dump stays human-readable even when payloads carry
//!     deeply nested objects (verdicts, evidence packs, …).
//!   - Non-ed25519 signature detection is centralized: any entry whose
//!     signature algo is not `ed25519` increments `non_ed25519_signature_count`.
//!
//! Wave 9 — given a verdict / subject id, reconstruct the full decision path
//! (intent → lease → reviews → verdict → merge passport → release passport →
//! rollback) from the append-only launch_ledger and emit a compact, signed
//! decision timeline. Used by operators investigating "why did this merge?".

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::ledger::{LedgerFilter, SqlLedger};
use super::types::{LaunchLedgerEntry, LedgerKind};

// ---------------------------------------------------------------------------
// Public surface
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ReplayReport {
    pub subject_id: String,
    pub entries_count: usize,
    pub timeline: Vec<TimelineEvent>,
    pub summary: ReplaySummary,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub at: DateTime<Utc>,
    pub kind: String,
    pub actor: String,
    pub id: String,
    pub payload_excerpt: serde_json::Value,
    pub signature_algo: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplaySummary {
    pub first_event_at: Option<DateTime<Utc>>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub elapsed_secs: Option<i64>,
    pub final_decision: Option<String>,
    pub had_human_intervention: bool,
    pub had_rejudge: bool,
    pub had_rollback: bool,
    pub had_kill_bell_event: bool,
    pub non_ed25519_signature_count: usize,
}

/// Reconstruct the decision path for a single subject_id (typically a
/// verdict id like `vgv_…`). Entries are pulled from the append-only
/// launch_ledger and sorted by `recorded_at ASC`. The summary is computed
/// inline from the timeline.
pub async fn replay_verdict(ledger: &SqlLedger, subject_id: &str) -> Result<ReplayReport> {
    replay_subject(ledger, subject_id, None).await
}

/// Same as [`replay_verdict`] but lets the caller scope the walk to a single
/// `repo` slug. Useful when the same subject id appears in multiple repos
/// (vanishingly rare in practice, but `LedgerFilter::repo` lets us be
/// explicit when it matters).
pub async fn replay_subject(
    ledger: &SqlLedger,
    subject_id: &str,
    repo: Option<&str>,
) -> Result<ReplayReport> {
    let filter = LedgerFilter {
        subject_id: Some(subject_id.to_string()),
        repo: repo.map(|s| s.to_string()),
        ..Default::default()
    };
    let entries = ledger
        .list(&filter)
        .await
        .with_context(|| format!("list launch_ledger for subject_id={subject_id}"))?;
    Ok(build_report(subject_id, entries, Utc::now()))
}

/// Render a compact ASCII timeline. Designed for `--no-json` operator output
/// (see `autonomy replay`). Stable line shape so CI can grep for anomalies.
pub fn render_human(report: &ReplayReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("Replay of subject {}\n", report.subject_id));
    out.push_str("───────────────────────────────────────\n");
    if report.timeline.is_empty() {
        out.push_str("(no events found)\n");
    } else {
        for ev in &report.timeline {
            // Best-effort: pluck the `decision` field for verdict_issued so the
            // line carries the most-load-bearing scalar. Otherwise leave blank.
            let trailing = match ev.kind.as_str() {
                "verdict_issued" | "review_completed" => ev
                    .payload_excerpt
                    .get("decision")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("   {s}"))
                    // Default::default() is the documented empty semantic here:
                    // missing decision field → no trailing text.
                    .unwrap_or_default(),
                _ => String::new(),
            };
            out.push_str(&format!(
                "{at}  {kind:<28}  {actor}{trailing}\n",
                at = ev.at.to_rfc3339(),
                kind = ev.kind,
                actor = ev.actor,
                trailing = trailing,
            ));
        }
    }
    let elapsed = report
        .summary
        .elapsed_secs
        .map(|s| format!("{s}s total"))
        // Fallback display string for the empty-timeline / single-event case
        // where elapsed cannot be computed; non-critical operator output.
        .unwrap_or_else(|| "no elapsed".to_string());
    let final_decision = report
        .summary
        .final_decision
        .as_deref()
        // Fallback display string when no VerdictIssued entry exists in the
        // timeline; non-critical operator output.
        .unwrap_or("no verdict");
    out.push_str(&format!(
        "\nSummary: {n} events, {elapsed}, final decision {final_decision}.\n",
        n = report.entries_count,
    ));
    let mut anomalies: Vec<&'static str> = Vec::new();
    if report.summary.had_human_intervention {
        anomalies.push("human_intervention");
    }
    if report.summary.had_rejudge {
        anomalies.push("rejudge");
    }
    if report.summary.had_rollback {
        anomalies.push("rollback");
    }
    if report.summary.had_kill_bell_event {
        anomalies.push("kill_bell");
    }
    if report.summary.non_ed25519_signature_count > 0 {
        anomalies.push("non_ed25519_signatures");
    }
    let anomaly_line = if anomalies.is_empty() {
        "none".to_string()
    } else {
        anomalies.join(", ")
    };
    out.push_str(&format!("Anomalies: {anomaly_line}.\n"));
    out
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn build_report(
    subject_id: &str,
    entries: Vec<LaunchLedgerEntry>,
    generated_at: DateTime<Utc>,
) -> ReplayReport {
    let timeline: Vec<TimelineEvent> = entries
        .iter()
        .map(|e| TimelineEvent {
            at: e.recorded_at,
            kind: kind_as_str(e.kind).to_string(),
            actor: e.actor.clone(),
            id: e.id.clone(),
            payload_excerpt: shallow_excerpt(&e.payload),
            signature_algo: e.signature.algo.clone(),
        })
        .collect();
    let summary = summarize(&entries, &timeline);
    ReplayReport {
        subject_id: subject_id.to_string(),
        entries_count: entries.len(),
        timeline,
        summary,
        generated_at,
    }
}

fn summarize(entries: &[LaunchLedgerEntry], timeline: &[TimelineEvent]) -> ReplaySummary {
    let first_event_at = entries.first().map(|e| e.recorded_at);
    let last_event_at = entries.last().map(|e| e.recorded_at);
    let elapsed_secs = match (first_event_at, last_event_at) {
        (Some(a), Some(b)) => Some((b - a).num_seconds()),
        _ => None,
    };
    // Final decision: walk back-to-front and grab the first verdict_issued's
    // `decision` field. Returns None if no verdict_issued exists or the
    // payload is missing that field.
    let final_decision = entries
        .iter()
        .rev()
        .find(|e| e.kind == LedgerKind::VerdictIssued)
        .and_then(|e| {
            e.payload
                .get("decision")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let had_human_intervention = entries
        .iter()
        .any(|e| e.kind == LedgerKind::HumanDecisionRecorded);
    let had_rejudge = entries
        .iter()
        .any(|e| e.kind == LedgerKind::MergePassportInvalidated);
    let had_rollback = entries
        .iter()
        .any(|e| e.kind == LedgerKind::RollbackInitiated);
    let had_kill_bell_event = entries.iter().any(|e| {
        matches!(
            e.kind,
            LedgerKind::KillBellEngaged | LedgerKind::KillBellResumed
        )
    });
    // Use the timeline (not entries) for the non-ed25519 sig count so we agree
    // with what the operator actually sees in the rendered output.
    let non_ed25519_signature_count = timeline
        .iter()
        .filter(|t| t.signature_algo != "ed25519")
        .count();
    ReplaySummary {
        first_event_at,
        last_event_at,
        elapsed_secs,
        final_decision,
        had_human_intervention,
        had_rejudge,
        had_rollback,
        had_kill_bell_event,
        non_ed25519_signature_count,
    }
}

/// Strip deeply nested payload fields to keep the audit dump compact. Keeps
/// first-level scalars; replaces nested objects with `{"_nested": true}` and
/// nested arrays with `["_array", <len>]`. Mirrors the verdict_store
/// "summary view" used by the dashboard.
fn shallow_excerpt(payload: &serde_json::Value) -> serde_json::Value {
    let Some(map) = payload.as_object() else {
        // Non-object payload (scalar, array, null): pass through as-is. Arrays
        // are summarized at the top level so big payloads don't blow up the
        // excerpt.
        return match payload {
            serde_json::Value::Array(a) => serde_json::json!(["_array", a.len()]),
            other => other.clone(),
        };
    };
    let mut out = serde_json::Map::with_capacity(map.len());
    for (k, v) in map.iter() {
        let trimmed = match v {
            serde_json::Value::Object(_) => serde_json::json!({"_nested": true}),
            serde_json::Value::Array(a) => serde_json::json!(["_array", a.len()]),
            other => other.clone(),
        };
        out.insert(k.clone(), trimmed);
    }
    serde_json::Value::Object(out)
}

fn kind_as_str(k: LedgerKind) -> &'static str {
    match k {
        LedgerKind::IntentDeclared => "intent_declared",
        LedgerKind::LeaseIssued => "lease_issued",
        LedgerKind::LeaseExpired => "lease_expired",
        LedgerKind::EvidencePackCreated => "evidence_pack_created",
        LedgerKind::ReviewStarted => "review_started",
        LedgerKind::ReviewCompleted => "review_completed",
        LedgerKind::VerdictIssued => "verdict_issued",
        LedgerKind::MergePassportIssued => "merge_passport_issued",
        LedgerKind::MergePassportConsumed => "merge_passport_consumed",
        LedgerKind::MergePassportInvalidated => "merge_passport_invalidated",
        LedgerKind::ReleasePassportIssued => "release_passport_issued",
        LedgerKind::DeploymentStarted => "deployment_started",
        LedgerKind::DeploymentPromoted => "deployment_promoted",
        LedgerKind::RollbackInitiated => "rollback_initiated",
        LedgerKind::RollbackCompleted => "rollback_completed",
        LedgerKind::HumanEscalationRequested => "human_escalation_requested",
        LedgerKind::HumanDecisionRecorded => "human_decision_recorded",
        LedgerKind::WebhookReceived => "webhook_received",
        LedgerKind::AutonomyPackEditProposed => "autonomy_pack_edit_proposed",
        LedgerKind::AutonomyPackEditMerged => "autonomy_pack_edit_merged",
        LedgerKind::KillBellEngaged => "kill_bell_engaged",
        LedgerKind::KillBellResumed => "kill_bell_resumed",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::EdSigningKey;
    use crate::autonomy::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};
    use crate::db::AnyPool;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use chrono::{Duration, TimeZone, Utc};

    // Schema installer lives in the db boundary (closes HLT-006); this file
    // no longer imports `sqlx::` directly.
    async fn fresh_db() -> AnyPool {
        fresh_autonomy_pool().await
    }

    fn mint_event(
        id: &str,
        kind: LedgerKind,
        payload: serde_json::Value,
        recorded_at: DateTime<Utc>,
    ) -> LaunchLedgerEntry {
        let key = EdSigningKey::generate("test-agent");
        let body = format!("{id}|{:?}", kind);
        let sig = key.sign_raw(body.as_bytes());
        LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: "vgv_abc".into(),
            repo: Some("owner/repo".into()),
            payload,
            recorded_at,
            actor: "agent-builder".into(),
            signature: sig,
        }
    }

    fn ts(year: i32, mon: u32, day: u32, hour: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, mon, day, hour, min, sec)
            .unwrap()
    }

    #[tokio::test]
    async fn replay_with_no_entries_returns_empty_timeline() {
        let ledger = SqlLedger::new(fresh_db().await);
        let report = replay_verdict(&ledger, "vgv_missing").await.unwrap();
        assert_eq!(report.subject_id, "vgv_missing");
        assert_eq!(report.entries_count, 0);
        assert!(report.timeline.is_empty());
        assert!(report.summary.first_event_at.is_none());
        assert!(report.summary.last_event_at.is_none());
        assert!(report.summary.elapsed_secs.is_none());
        assert!(report.summary.final_decision.is_none());
    }

    #[tokio::test]
    async fn replay_orders_events_by_recorded_at_ascending() {
        let ledger = SqlLedger::new(fresh_db().await);
        // Append in reverse chronological order; the ledger's ORDER BY ASC
        // must put them back in wall-clock order.
        ledger
            .append(&mint_event(
                "evt-late",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 5, 0),
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-early",
                LedgerKind::IntentDeclared,
                serde_json::json!({"summary": "fix bug"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(report.entries_count, 2);
        assert_eq!(report.timeline[0].id, "evt-early");
        assert_eq!(report.timeline[1].id, "evt-late");
        assert!(report.timeline[0].at < report.timeline[1].at);
    }

    #[tokio::test]
    async fn replay_computes_elapsed_secs_from_first_to_last() {
        let ledger = SqlLedger::new(fresh_db().await);
        let t0 = ts(2026, 5, 16, 14, 0, 0);
        let t1 = t0 + Duration::seconds(121);
        ledger
            .append(&mint_event(
                "evt-a",
                LedgerKind::IntentDeclared,
                serde_json::json!({}),
                t0,
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-b",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                t1,
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(report.summary.elapsed_secs, Some(121));
        assert_eq!(report.summary.first_event_at, Some(t0));
        assert_eq!(report.summary.last_event_at, Some(t1));
    }

    #[tokio::test]
    async fn replay_extracts_final_decision_from_verdict_payload() {
        let ledger = SqlLedger::new(fresh_db().await);
        // Two verdict_issued entries — the latest one wins.
        ledger
            .append(&mint_event(
                "evt-v1",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "require_human"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-v2",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 5, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(
            report.summary.final_decision.as_deref(),
            Some("allow_merge")
        );
    }

    #[tokio::test]
    async fn replay_flags_had_human_intervention_on_human_decision_recorded() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-hd",
                LedgerKind::HumanDecisionRecorded,
                serde_json::json!({"decided_by": "ops@example.com"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert!(report.summary.had_human_intervention);
        assert!(!report.summary.had_rejudge);
        assert!(!report.summary.had_rollback);
    }

    #[tokio::test]
    async fn replay_flags_had_rejudge_on_merge_passport_invalidated() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-inv",
                LedgerKind::MergePassportInvalidated,
                serde_json::json!({"reason": "head_sha_drift"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert!(report.summary.had_rejudge);
    }

    #[tokio::test]
    async fn replay_flags_had_rollback_on_rollback_initiated() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-rb",
                LedgerKind::RollbackInitiated,
                serde_json::json!({"strategy": "revert_commit"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert!(report.summary.had_rollback);
    }

    #[tokio::test]
    async fn replay_flags_had_kill_bell_event_on_kill_bell_engaged() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-kb",
                LedgerKind::KillBellEngaged,
                serde_json::json!({"reason": "incident"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert!(report.summary.had_kill_bell_event);
    }

    #[tokio::test]
    async fn replay_counts_stale_signatures() {
        // We can't go through SqlLedger::append (it refuses stub signatures
        // outright). Insert directly via raw SQL to simulate a legacy entry
        // that landed before ed25519 was enforced.
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        // One legit ed25519 entry, one stub-signed entry that's parseable.
        ledger
            .append(&mint_event(
                "evt-good",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        crate::db::raw_query(
            "INSERT INTO launch_ledger
                 (id, kind, subject_id, repo, actor, payload,
                  signature_algo, signature_key_id, signature_value, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("evt-stale")
        .bind("review_completed")
        .bind("vgv_abc")
        .bind::<Option<&str>>(Some("owner/repo"))
        .bind("legacy-agent")
        .bind("{}")
        .bind("sha256-hmac-stub")
        .bind("legacy-key")
        .bind("0".repeat(64))
        .bind(ts(2026, 5, 16, 13, 59, 0).to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(report.entries_count, 2);
        assert_eq!(report.summary.non_ed25519_signature_count, 1);
    }

    #[tokio::test]
    async fn replay_subject_filters_by_repo_when_provided() {
        let ledger = SqlLedger::new(fresh_db().await);
        let mut a = mint_event(
            "evt-r1",
            LedgerKind::VerdictIssued,
            serde_json::json!({"decision": "allow_merge"}),
            ts(2026, 5, 16, 14, 0, 0),
        );
        a.repo = Some("owner/repo-A".into());
        let mut b = mint_event(
            "evt-r2",
            LedgerKind::VerdictIssued,
            serde_json::json!({"decision": "reject"}),
            ts(2026, 5, 16, 14, 1, 0),
        );
        b.repo = Some("owner/repo-B".into());
        ledger.append(&a).await.unwrap();
        ledger.append(&b).await.unwrap();

        let report_a = replay_subject(&ledger, "vgv_abc", Some("owner/repo-A"))
            .await
            .unwrap();
        assert_eq!(report_a.entries_count, 1);
        assert_eq!(report_a.timeline[0].id, "evt-r1");
        assert_eq!(
            report_a.summary.final_decision.as_deref(),
            Some("allow_merge")
        );

        let report_b = replay_subject(&ledger, "vgv_abc", Some("owner/repo-B"))
            .await
            .unwrap();
        assert_eq!(report_b.entries_count, 1);
        assert_eq!(report_b.timeline[0].id, "evt-r2");
        assert_eq!(report_b.summary.final_decision.as_deref(), Some("reject"));

        // No repo filter — both entries.
        let report_all = replay_subject(&ledger, "vgv_abc", None).await.unwrap();
        assert_eq!(report_all.entries_count, 2);
    }

    #[tokio::test]
    async fn payload_excerpt_strips_deeply_nested_objects() {
        let ledger = SqlLedger::new(fresh_db().await);
        // A 3-level-deep payload — the excerpt should flatten anything past
        // level 1 into `{"_nested": true}` / `["_array", n]`.
        let deep = serde_json::json!({
            "decision": "allow_merge",
            "approval_receipts": [
                {"role": "security", "agent_id": "sec.v1"},
                {"role": "judge",    "agent_id": "judge.v1"}
            ],
            "judge_context": {
                "policy": {"sha": "c".repeat(40), "version": "v1"},
                "thresholds": {"high": 5, "critical": 0}
            }
        });
        ledger
            .append(&mint_event(
                "evt-deep",
                LedgerKind::VerdictIssued,
                deep,
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        let excerpt = &report.timeline[0].payload_excerpt;
        // First-level scalar survives.
        assert_eq!(
            excerpt.get("decision").and_then(|v| v.as_str()),
            Some("allow_merge")
        );
        // Arrays collapse to ["_array", N].
        let receipts = excerpt.get("approval_receipts").unwrap();
        assert_eq!(receipts[0], serde_json::json!("_array"));
        assert_eq!(receipts[1], serde_json::json!(2));
        // Nested objects collapse to {"_nested": true}.
        let ctx = excerpt.get("judge_context").unwrap();
        assert_eq!(ctx, &serde_json::json!({"_nested": true}));
        // Crucially, the deep grand-children must not appear anywhere in the
        // excerpt's serialized form.
        let dumped = serde_json::to_string(excerpt).unwrap();
        assert!(!dumped.contains("thresholds"));
        assert!(!dumped.contains("sec.v1"));
    }

    #[tokio::test]
    async fn render_human_includes_subject_id_and_anomaly_summary() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-1",
                LedgerKind::IntentDeclared,
                serde_json::json!({"summary": "test intent"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-2",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 2, 0),
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-3",
                LedgerKind::RollbackInitiated,
                serde_json::json!({"strategy": "revert_commit"}),
                ts(2026, 5, 16, 14, 10, 0),
            ))
            .await
            .unwrap();

        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        let rendered = render_human(&report);
        assert!(
            rendered.contains("vgv_abc"),
            "subject id must appear: {rendered}"
        );
        assert!(rendered.contains("intent_declared"));
        assert!(rendered.contains("verdict_issued"));
        assert!(rendered.contains("rollback_initiated"));
        assert!(rendered.contains("allow_merge"));
        assert!(rendered.contains("Anomalies"));
        assert!(rendered.contains("rollback"));
    }

    #[tokio::test]
    async fn replay_with_single_verdict_issued_summary_is_compact() {
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-only",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "reject"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(report.entries_count, 1);
        assert_eq!(report.summary.elapsed_secs, Some(0));
        assert_eq!(report.summary.final_decision.as_deref(), Some("reject"));
        assert!(!report.summary.had_human_intervention);
        assert!(!report.summary.had_rejudge);
        assert!(!report.summary.had_rollback);
        assert_eq!(report.summary.non_ed25519_signature_count, 0);
        let rendered = render_human(&report);
        assert!(
            rendered.contains("none"),
            "anomaly line should be 'none': {rendered}"
        );
    }

    #[tokio::test]
    async fn render_human_empty_timeline_still_prints_header() {
        let ledger = SqlLedger::new(fresh_db().await);
        let report = replay_verdict(&ledger, "vgv_nope").await.unwrap();
        let rendered = render_human(&report);
        assert!(rendered.contains("Replay of subject vgv_nope"));
        assert!(rendered.contains("(no events found)"));
        assert!(rendered.contains("Anomalies: none"));
    }

    /// Wave 10 mint — `WebhookReceived` entries land in the replay timeline
    /// with kind `"webhook_received"`. If `kind_as_str` ever forgets the new
    /// variant the timeline would carry a stale or wrong label and the
    /// audit dump would silently misclassify webhook events. This test
    /// pins both the timeline kind string and the entries_count.
    #[tokio::test]
    async fn replay_timeline_includes_webhook_received_events() {
        let ledger = SqlLedger::new(fresh_db().await);
        // Two ordering markers (a verdict + a webhook) so we can also
        // verify the webhook lands distinct from any human-decision event.
        ledger
            .append(&mint_event(
                "evt-wh",
                LedgerKind::WebhookReceived,
                serde_json::json!({
                    "event_type": "pull_request",
                    "action": "opened",
                    "pr_number": 42,
                }),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        ledger
            .append(&mint_event(
                "evt-v1",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 5, 0),
            ))
            .await
            .unwrap();

        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        assert_eq!(report.entries_count, 2);
        // The webhook must show up by snake_case kind name on the timeline.
        let webhook_events: Vec<_> = report
            .timeline
            .iter()
            .filter(|e| e.kind == "webhook_received")
            .collect();
        assert_eq!(
            webhook_events.len(),
            1,
            "exactly one webhook_received entry must surface in the timeline"
        );
        assert_eq!(webhook_events[0].id, "evt-wh");
        // First-level scalar payload fields survive `shallow_excerpt`.
        assert_eq!(
            webhook_events[0]
                .payload_excerpt
                .get("pr_number")
                .and_then(|v| v.as_u64()),
            Some(42)
        );
        // Webhook events must NOT trip `had_human_intervention` — that's
        // the whole point of minting the dedicated kind.
        assert!(
            !report.summary.had_human_intervention,
            "webhook_received must not trip had_human_intervention; \
             that flag is reserved for HumanDecisionRecorded"
        );
    }

    #[tokio::test]
    async fn replay_report_serializes_to_json() {
        // Wave 9 promises a `--json` mode in the CLI; verify the report is
        // serializable to a non-trivial JSON object so the bin layer can call
        // `to_string_pretty` without surprises.
        let ledger = SqlLedger::new(fresh_db().await);
        ledger
            .append(&mint_event(
                "evt-1",
                LedgerKind::VerdictIssued,
                serde_json::json!({"decision": "allow_merge"}),
                ts(2026, 5, 16, 14, 0, 0),
            ))
            .await
            .unwrap();
        let report = replay_verdict(&ledger, "vgv_abc").await.unwrap();
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["subject_id"], "vgv_abc");
        assert_eq!(json["entries_count"], 1);
        assert!(json["timeline"].is_array());
        assert_eq!(json["timeline"][0]["kind"], "verdict_issued");
        assert_eq!(json["summary"]["final_decision"], "allow_merge");
    }
}
