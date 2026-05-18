//! Owner: Evidence Gate / autonomy control plane (Wave 5)
//! Proof: `cargo test -p jeryu --lib autonomy::metrics`
//! Invariants:
//!   - The exporter is a pull-based snapshot: every `collect()` call reads
//!     fresh data from the `launch_ledger` + `kill_bell_state` tables. There
//!     is no background ticker and no in-process counter aggregation; this
//!     keeps the surface stateless and trivially restart-safe (Wave 5 brief).
//!   - `render_prometheus()` MUST emit exactly one `# HELP` and one `# TYPE`
//!     line per metric name. Prometheus's text format rejects duplicates and
//!     scrapers will silently drop the metric — so the rendering pipeline
//!     groups samples by name and writes the header once.
//!   - No new crate dependencies. Prometheus's text format is small enough
//!     to hand-roll; pulling `prometheus`/`metrics` would bloat the binary
//!     and create a transitive surface area we'd then have to police.
//!
//! Brainstorm refs: `tips/fullauto/tip1.txt` + Evidence Gate Wave 5 brief —
//! before operators trust the autonomous-delivery control plane in
//! production they need observability into promotion / rollback / kill-bell
//! / verdict cadence. This module surfaces the ~15 metrics enumerated in
//! the brainstorm.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::collections::BTreeMap;

use super::kill_bell::{KillBell, KillBellState};
use super::ledger::{LedgerFilter, SqlLedger};
use super::types::{LaunchLedgerEntry, LedgerKind};

/// Histogram buckets (seconds) for `jeryu_promotion_to_rollback_seconds`.
/// Chosen to span 1 minute → 6 hours; +Inf is implicit and rendered last.
const ROLLBACK_BUCKETS: &[f64] = &[60.0, 300.0, 900.0, 3600.0, 21600.0];

/// Snapshot returned by `collect()`. Owns the data; callers render it.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub generated_at: DateTime<Utc>,
    pub counters: Vec<Counter>,
    pub gauges: Vec<Gauge>,
    pub histograms: Vec<Histogram>,
}

#[derive(Debug, Clone)]
pub struct Counter {
    pub name: String,
    pub help: String,
    pub labels: Vec<(String, String)>,
    pub value: u64,
}

#[derive(Debug, Clone)]
pub struct Gauge {
    pub name: String,
    pub help: String,
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct Histogram {
    pub name: String,
    pub help: String,
    pub labels: Vec<(String, String)>,
    /// Cumulative bucket counts keyed by upper-bound (seconds). The +Inf
    /// bucket is implicit: `count` already equals the +Inf cumulative.
    pub buckets: Vec<(f64, u64)>,
    pub sum: f64,
    pub count: u64,
}

/// Pull all metrics from the SQL surfaces. Snapshot-only — no background work.
#[allow(clippy::vec_init_then_push)]
pub async fn collect(
    ledger: &SqlLedger,
    kill_bell: &KillBell,
    now: DateTime<Utc>,
) -> Result<MetricsSnapshot> {
    // One unfiltered list keeps the SQL surface to a single round-trip;
    // we partition in-process. Cheap because the ledger row count is bounded
    // by operator activity (not request volume).
    let all = ledger.list(&LedgerFilter::default()).await?;

    let mut counters = Vec::new();

    counters.push(Counter {
        name: "jeryu_autonomous_promotion_total".into(),
        help:
            "Count of DeploymentPromoted ledger entries (successful autonomous promotions to prod)."
                .into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::DeploymentPromoted),
    });

    counters.push(Counter {
        name: "jeryu_nightwatch_rollback_total".into(),
        help: "Count of RollbackInitiated ledger entries attributed to the Nightwatch reviewer (actor prefix or payload.reason match).".into(),
        labels: vec![],
        value: count_nightwatch_rollbacks(&all),
    });

    counters.push(Counter {
        name: "jeryu_kill_bell_engaged_total".into(),
        help: "Count of KillBellEngaged ledger entries (global pause activations).".into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::KillBellEngaged),
    });

    counters.push(Counter {
        name: "jeryu_kill_bell_resumed_total".into(),
        help:
            "Count of KillBellResumed ledger entries (operator-initiated or TTL auto-arm resumes)."
                .into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::KillBellResumed),
    });

    // jeryu_verdict_issued_total{decision="..."}. We materialize one
    // Counter per observed decision label so each row in the rendered
    // text format carries the same metric name + a distinct label set.
    let mut by_decision: BTreeMap<String, u64> = BTreeMap::new();
    for e in all.iter().filter(|e| e.kind == LedgerKind::VerdictIssued) {
        let decision = extract_decision_label(&e.payload);
        *by_decision.entry(decision).or_insert(0) += 1;
    }
    if by_decision.is_empty() {
        // Emit a zero-valued series with decision="unknown" so the metric
        // name is always present in the scrape — Prometheus alerts that
        // reference an absent series fire as `no data` which is noise.
        counters.push(Counter {
            name: "jeryu_verdict_issued_total".into(),
            help: "Count of VerdictIssued ledger entries, labeled by the verdict's decision string (best-effort payload extraction).".into(),
            labels: vec![("decision".into(), "unknown".into())],
            value: 0,
        });
    } else {
        for (decision, value) in by_decision {
            counters.push(Counter {
                name: "jeryu_verdict_issued_total".into(),
                help: "Count of VerdictIssued ledger entries, labeled by the verdict's decision string (best-effort payload extraction).".into(),
                labels: vec![("decision".into(), decision)],
                value,
            });
        }
    }

    counters.push(Counter {
        name: "jeryu_human_escalation_total".into(),
        help: "Count of HumanEscalationRequested ledger entries (gate downgrades requiring human review).".into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::HumanEscalationRequested),
    });

    // Wave 10 mint — dedicated counter for verified inbound webhooks so the
    // `jeryu_human_escalation_total` series stays clean (webhooks used to
    // reuse `HumanDecisionRecorded`).
    counters.push(Counter {
        name: "jeryu_webhook_received_total".into(),
        help:
            "Count of WebhookReceived ledger entries (verified inbound webhooks on POST /events)."
                .into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::WebhookReceived),
    });

    counters.push(Counter {
        name: "jeryu_evidence_pack_created_total".into(),
        help: "Count of EvidencePackCreated ledger entries.".into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::EvidencePackCreated),
    });

    counters.push(Counter {
        name: "jeryu_merge_passport_invalidated_total".into(),
        help: "Count of MergePassportInvalidated ledger entries (passports voided before consumption).".into(),
        labels: vec![],
        value: count_kind(&all, LedgerKind::MergePassportInvalidated),
    });

    // Gauges --------------------------------------------------------------
    let mut gauges = Vec::new();

    let bell_state = kill_bell.current(now).await?;
    gauges.push(Gauge {
        name: "jeryu_kill_bell_state".into(),
        help: "Current Kill Bell posture: 0 = Armed (autonomous decisions flow), 1 = Paused (all decisions downgrade to RequireHuman).".into(),
        labels: vec![],
        value: match bell_state {
            KillBellState::Armed => 0.0,
            KillBellState::Paused { .. } => 1.0,
        },
    });

    let cutoff = now - Duration::hours(24);
    let last_24h = all.iter().filter(|e| e.recorded_at > cutoff).count() as f64;
    gauges.push(Gauge {
        name: "jeryu_ledger_entries_last_24h".into(),
        help: "Count of launch_ledger entries with recorded_at within the last 24 hours of the snapshot time.".into(),
        labels: vec![],
        value: last_24h,
    });

    let pairs = recent_rollback_pairs(&all, 10);
    let mttr_seconds = if pairs.is_empty() {
        0.0
    } else {
        let sum: f64 = pairs.iter().map(|d| *d as f64).sum();
        sum / pairs.len() as f64
    };
    gauges.push(Gauge {
        name: "jeryu_mean_time_to_rollback_seconds".into(),
        help: "Mean DeploymentPromoted->RollbackInitiated wall time (seconds) over the most-recent 10 matched-subject pairs; 0 if none.".into(),
        labels: vec![],
        value: mttr_seconds,
    });

    // Histograms ----------------------------------------------------------
    let histogram = histogram_from_pairs(&pairs);
    let histograms = vec![Histogram {
        name: "jeryu_promotion_to_rollback_seconds".into(),
        help: "Distribution of DeploymentPromoted->RollbackInitiated wall time (seconds) over the most-recent 10 matched-subject pairs.".into(),
        labels: vec![],
        buckets: histogram.0,
        sum: histogram.1,
        count: histogram.2,
    }];

    Ok(MetricsSnapshot {
        generated_at: now,
        counters,
        gauges,
        histograms,
    })
}

/// Render the snapshot as Prometheus text format (v0.0.4 / OpenMetrics-compat
/// subset). The Prometheus parser expects exactly one `# HELP` + `# TYPE`
/// per metric name, then any number of sample lines. We group by name to
/// honor that contract regardless of how counters are pushed.
pub fn render_prometheus(snap: &MetricsSnapshot) -> String {
    let mut out = String::new();

    // Group counters by name so each metric name gets one HELP+TYPE block.
    let mut counter_groups: BTreeMap<String, (String, Vec<&Counter>)> = BTreeMap::new();
    for c in &snap.counters {
        counter_groups
            .entry(c.name.clone())
            .or_insert_with(|| (c.help.clone(), Vec::new()))
            .1
            .push(c);
    }
    for (name, (help, items)) in counter_groups {
        out.push_str(&format!("# HELP {} {}\n", name, escape_help(&help)));
        out.push_str(&format!("# TYPE {} counter\n", name));
        for c in items {
            out.push_str(&render_sample(&name, &c.labels, c.value as f64));
        }
    }

    let mut gauge_groups: BTreeMap<String, (String, Vec<&Gauge>)> = BTreeMap::new();
    for g in &snap.gauges {
        gauge_groups
            .entry(g.name.clone())
            .or_insert_with(|| (g.help.clone(), Vec::new()))
            .1
            .push(g);
    }
    for (name, (help, items)) in gauge_groups {
        out.push_str(&format!("# HELP {} {}\n", name, escape_help(&help)));
        out.push_str(&format!("# TYPE {} gauge\n", name));
        for g in items {
            out.push_str(&render_sample(&name, &g.labels, g.value));
        }
    }

    let mut hist_groups: BTreeMap<String, (String, Vec<&Histogram>)> = BTreeMap::new();
    for h in &snap.histograms {
        hist_groups
            .entry(h.name.clone())
            .or_insert_with(|| (h.help.clone(), Vec::new()))
            .1
            .push(h);
    }
    for (name, (help, items)) in hist_groups {
        out.push_str(&format!("# HELP {} {}\n", name, escape_help(&help)));
        out.push_str(&format!("# TYPE {} histogram\n", name));
        for h in items {
            for (upper, cumulative) in &h.buckets {
                let mut labels = h.labels.clone();
                labels.push(("le".into(), format_float(*upper)));
                out.push_str(&render_sample(
                    &format!("{}_bucket", name),
                    &labels,
                    *cumulative as f64,
                ));
            }
            // +Inf bucket equals count.
            let mut inf_labels = h.labels.clone();
            inf_labels.push(("le".into(), "+Inf".into()));
            out.push_str(&render_sample(
                &format!("{}_bucket", name),
                &inf_labels,
                h.count as f64,
            ));
            out.push_str(&render_sample(&format!("{}_sum", name), &h.labels, h.sum));
            out.push_str(&render_sample(
                &format!("{}_count", name),
                &h.labels,
                h.count as f64,
            ));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

fn count_kind(entries: &[LaunchLedgerEntry], kind: LedgerKind) -> u64 {
    entries.iter().filter(|e| e.kind == kind).count() as u64
}

/// Count rollbacks attributed to Nightwatch via either the actor prefix
/// or a payload.reason substring. The brainstorm allows either signal so
/// we OR them — false positives here are cheap; missed counts erode trust.
fn count_nightwatch_rollbacks(entries: &[LaunchLedgerEntry]) -> u64 {
    entries
        .iter()
        .filter(|e| e.kind == LedgerKind::RollbackInitiated)
        .filter(|e| {
            if e.actor.starts_with("reviewer-nightwatch") {
                return true;
            }
            if let Some(reason) = e.payload.get("reason").and_then(|v| v.as_str())
                && reason.contains("nightwatch")
            {
                return true;
            }
            false
        })
        .count() as u64
}

/// Best-effort extraction of `payload.decision`. Falls back to "unknown"
/// so we never drop a sample on malformed payloads (the rollup is more
/// useful than zero data, and the "unknown" bucket itself is a signal).
fn extract_decision_label(payload: &serde_json::Value) -> String {
    payload
        .get("decision")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Compute durations (seconds) for the most-recent `limit` paired
/// DeploymentPromoted -> RollbackInitiated events, keyed by `subject_id`.
/// Returns oldest-first because the caller treats this as a sample set.
/// Pairs are formed greedily: the earliest unmatched promotion for the
/// subject pairs with the next rollback for the same subject.
fn recent_rollback_pairs(entries: &[LaunchLedgerEntry], limit: usize) -> Vec<i64> {
    use std::collections::HashMap;
    // entries are oldest-first per SqlLedger::list contract.
    let mut promo_by_subject: HashMap<&str, Vec<DateTime<Utc>>> = HashMap::new();
    let mut pairs: Vec<i64> = Vec::new();

    for e in entries {
        match e.kind {
            LedgerKind::DeploymentPromoted => {
                promo_by_subject
                    .entry(e.subject_id.as_str())
                    .or_default()
                    .push(e.recorded_at);
            }
            LedgerKind::RollbackInitiated => {
                if let Some(queue) = promo_by_subject.get_mut(e.subject_id.as_str())
                    && !queue.is_empty()
                {
                    let promoted_at = queue.remove(0);
                    let delta = (e.recorded_at - promoted_at).num_seconds();
                    // Negative deltas (clock skew, out-of-order ingest)
                    // would skew MTTR; clamp at 0.
                    pairs.push(delta.max(0));
                }
            }
            _ => {}
        }
    }

    // Keep the most-recent `limit` pairs. We appended in time order so the
    // tail is newest; truncate from the front if we exceed the cap.
    if pairs.len() > limit {
        let drop = pairs.len() - limit;
        pairs.drain(0..drop);
    }
    pairs
}

/// Build (cumulative buckets, sum, count) for the histogram from a flat
/// vector of durations. Returns explicit-bound buckets only; the +Inf bucket
/// is rendered by `render_prometheus` from `count`.
fn histogram_from_pairs(pairs: &[i64]) -> (Vec<(f64, u64)>, f64, u64) {
    let mut buckets: Vec<(f64, u64)> = ROLLBACK_BUCKETS.iter().map(|b| (*b, 0)).collect();
    let mut sum = 0.0f64;
    for d in pairs {
        let v = *d as f64;
        sum += v;
        for (upper, count) in buckets.iter_mut() {
            if v <= *upper {
                *count += 1;
            }
        }
    }
    (buckets, sum, pairs.len() as u64)
}

fn render_sample(name: &str, labels: &[(String, String)], value: f64) -> String {
    if labels.is_empty() {
        format!("{} {}\n", name, format_float(value))
    } else {
        let label_str = labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, escape_label(v)))
            .collect::<Vec<_>>()
            .join(",");
        format!("{}{{{}}} {}\n", name, label_str, format_float(value))
    }
}

/// Help text MUST escape backslashes and newlines per the Prometheus
/// text-format spec; everything else is opaque.
fn escape_help(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

/// Label values escape backslashes, double quotes, and newlines.
fn escape_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Integer-valued floats render without a trailing `.0` to stay
/// byte-equivalent to typical Prometheus output (`42` not `42.0`).
fn format_float(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e18 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::ledger::{sign_entry, verdict_issued_entry};
    use crate::autonomy::signing::{EdSigningKey, Signature};
    use crate::autonomy::types::{
        GateDecision, LaunchLedgerEntry, LedgerKind, RiskTier, SchemaTag, VerdictReceiptRef,
        VibeGateVerdict,
    };
    use crate::db::AnyPool;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use chrono::{Duration, Utc};

    /// In-memory SQLite pool. Schema installer lives in the db boundary
    /// (closes HLT-006): this file no longer imports `sqlx::`.
    async fn fresh_db() -> AnyPool {
        fresh_autonomy_pool().await
    }

    fn key() -> EdSigningKey {
        EdSigningKey::generate("metrics-test")
    }

    /// Build an arbitrary signed entry for `kind`. Subject is unique by
    /// `id` so callers can mint multiple distinct rows without colliding
    /// on the append-once primary key.
    fn make_entry(
        id: &str,
        kind: LedgerKind,
        subject_id: &str,
        actor: &str,
        payload: serde_json::Value,
        recorded_at: chrono::DateTime<Utc>,
    ) -> LaunchLedgerEntry {
        let mut e = LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: subject_id.into(),
            repo: Some("owner/repo".into()),
            payload,
            recorded_at,
            actor: actor.into(),
            signature: Signature::stub(),
        };
        sign_entry(&mut e, &key());
        e
    }

    #[tokio::test]
    async fn collect_with_empty_ledger_returns_zero_counters() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let snap = collect(&ledger, &bell, Utc::now()).await.unwrap();

        // Every counter must be present (zero-valued) so dashboards/alerts
        // see the metric series instead of "no data" gaps.
        let names: Vec<&str> = snap.counters.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"jeryu_autonomous_promotion_total"));
        assert!(names.contains(&"jeryu_nightwatch_rollback_total"));
        assert!(names.contains(&"jeryu_kill_bell_engaged_total"));
        assert!(names.contains(&"jeryu_kill_bell_resumed_total"));
        assert!(names.contains(&"jeryu_human_escalation_total"));
        assert!(names.contains(&"jeryu_evidence_pack_created_total"));
        assert!(names.contains(&"jeryu_merge_passport_invalidated_total"));
        assert!(names.contains(&"jeryu_verdict_issued_total"));

        for c in &snap.counters {
            assert_eq!(c.value, 0, "{} should be 0 on empty ledger", c.name);
        }
    }

    #[tokio::test]
    async fn collect_counts_kill_bell_engaged() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool.clone());
        let now = Utc::now();
        // Engage the bell via the real API so we exercise the same write
        // path production uses (signed ledger entry + state row).
        bell.pause("a", "alice", 60, &key(), now).await.unwrap();
        bell.resume("alice", &key(), now + Duration::seconds(5))
            .await
            .unwrap();
        bell.pause("b", "bob", 60, &key(), now + Duration::seconds(10))
            .await
            .unwrap();

        let snap = collect(&ledger, &bell, now + Duration::seconds(15))
            .await
            .unwrap();
        let engaged = snap
            .counters
            .iter()
            .find(|c| c.name == "jeryu_kill_bell_engaged_total")
            .expect("counter missing");
        assert_eq!(engaged.value, 2);
        let resumed = snap
            .counters
            .iter()
            .find(|c| c.name == "jeryu_kill_bell_resumed_total")
            .expect("counter missing");
        assert_eq!(resumed.value, 1);
    }

    #[tokio::test]
    async fn collect_counts_verdict_issued_by_decision_label() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let k = key();
        let now = Utc::now();

        // Mint three verdicts: 2 allow_merge, 1 reject. We re-use
        // `verdict_issued_entry()` so the payload shape matches the real
        // production-path JSON the exporter must parse.
        for (idx, decision) in [
            GateDecision::AllowMerge,
            GateDecision::AllowMerge,
            GateDecision::Reject,
        ]
        .iter()
        .enumerate()
        {
            let verdict = VibeGateVerdict {
                schema: SchemaTag::new(),
                id: format!("vgv_{idx}"),
                evidence_pack_id: "ep".into(),
                merge_request: None,
                repo: "owner/repo".into(),
                target_branch: "main".into(),
                head_sha: "a".repeat(40),
                policy_sha: "c".repeat(40),
                evidence_pack_digest: "sha256:deadbeef".into(),
                risk: RiskTier::R2,
                hard_stops: vec![],
                required_reviews: vec![],
                approval_receipts: Vec::<VerdictReceiptRef>::new(),
                decision: *decision,
                valid_for_head_sha_only: true,
                rebind_on_train: true,
                expires_at: now + Duration::minutes(60),
                created_at: now,
                signature: Signature::stub(),
            };
            let mut entry = verdict_issued_entry(&verdict, "judge.v1");
            // verdict_issued_entry routes RequireHuman to HumanEscalationRequested;
            // for AllowMerge and Reject the kind is VerdictIssued — which is
            // exactly what this metric counts.
            sign_entry(&mut entry, &k);
            ledger.append(&entry).await.unwrap();
        }

        let snap = collect(&ledger, &bell, now).await.unwrap();
        let allow = snap
            .counters
            .iter()
            .find(|c| {
                c.name == "jeryu_verdict_issued_total"
                    && c.labels
                        .iter()
                        .any(|(k, v)| k == "decision" && v == "allow_merge")
            })
            .expect("allow_merge series missing");
        assert_eq!(allow.value, 2);
        let reject = snap
            .counters
            .iter()
            .find(|c| {
                c.name == "jeryu_verdict_issued_total"
                    && c.labels
                        .iter()
                        .any(|(k, v)| k == "decision" && v == "reject")
            })
            .expect("reject series missing");
        assert_eq!(reject.value, 1);
    }

    #[tokio::test]
    async fn collect_returns_gauge_0_when_kill_bell_armed() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let snap = collect(&ledger, &bell, Utc::now()).await.unwrap();
        let g = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_kill_bell_state")
            .expect("kill bell gauge missing");
        assert_eq!(g.value, 0.0);
    }

    #[tokio::test]
    async fn collect_returns_gauge_1_when_kill_bell_paused() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool.clone());
        let now = Utc::now();
        bell.pause("audit", "alice", 3600, &key(), now)
            .await
            .unwrap();
        let snap = collect(&ledger, &bell, now).await.unwrap();
        let g = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_kill_bell_state")
            .expect("kill bell gauge missing");
        assert_eq!(g.value, 1.0);
    }

    #[tokio::test]
    async fn mttr_seconds_is_zero_without_rollback_pairs() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let now = Utc::now();
        // Promotions with no matching rollbacks: MTTR must be 0 (not NaN).
        ledger
            .append(&make_entry(
                "p1",
                LedgerKind::DeploymentPromoted,
                "rel-1",
                "release.v1",
                serde_json::json!({}),
                now,
            ))
            .await
            .unwrap();
        let snap = collect(&ledger, &bell, now + Duration::seconds(10))
            .await
            .unwrap();
        let mttr = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_mean_time_to_rollback_seconds")
            .expect("mttr gauge missing");
        assert_eq!(mttr.value, 0.0);
    }

    #[tokio::test]
    async fn mttr_seconds_computes_mean_from_paired_events() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let base = Utc::now();

        // Pair 1: rel-A promoted @ t0, rolled back @ t0+120s -> 120s
        ledger
            .append(&make_entry(
                "promote-a",
                LedgerKind::DeploymentPromoted,
                "rel-A",
                "release.v1",
                serde_json::json!({}),
                base,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "rollback-a",
                LedgerKind::RollbackInitiated,
                "rel-A",
                "release.v1",
                serde_json::json!({"reason": "user-reported regression"}),
                base + Duration::seconds(120),
            ))
            .await
            .unwrap();
        // Pair 2: rel-B promoted @ t0+200s, rolled back @ t0+500s -> 300s
        ledger
            .append(&make_entry(
                "promote-b",
                LedgerKind::DeploymentPromoted,
                "rel-B",
                "release.v1",
                serde_json::json!({}),
                base + Duration::seconds(200),
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "rollback-b",
                LedgerKind::RollbackInitiated,
                "rel-B",
                "release.v1",
                serde_json::json!({}),
                base + Duration::seconds(500),
            ))
            .await
            .unwrap();

        let snap = collect(&ledger, &bell, base + Duration::seconds(600))
            .await
            .unwrap();
        let mttr = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_mean_time_to_rollback_seconds")
            .expect("mttr gauge missing");
        assert_eq!(mttr.value, 210.0, "mean of 120 and 300 should be 210");

        // The histogram should reflect the same two samples.
        let h = snap
            .histograms
            .iter()
            .find(|h| h.name == "jeryu_promotion_to_rollback_seconds")
            .expect("histogram missing");
        assert_eq!(h.count, 2);
        assert_eq!(h.sum, 420.0);
        // Both samples (120 and 300) fall into the le=300 bucket and beyond.
        let le_300 = h
            .buckets
            .iter()
            .find(|(b, _)| *b == 300.0)
            .map(|(_, c)| *c)
            .expect("le=300 bucket missing");
        assert_eq!(le_300, 2);
        let le_60 = h
            .buckets
            .iter()
            .find(|(b, _)| *b == 60.0)
            .map(|(_, c)| *c)
            .expect("le=60 bucket missing");
        assert_eq!(le_60, 0);
    }

    #[tokio::test]
    async fn render_prometheus_produces_valid_text_format() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool.clone());
        let now = Utc::now();

        // Seed at least one of every category so the render path covers
        // counter+gauge+histogram and the de-dup logic for the labeled
        // verdict counter.
        bell.pause("seed", "alice", 60, &key(), now).await.unwrap();
        ledger
            .append(&make_entry(
                "ev-1",
                LedgerKind::EvidencePackCreated,
                "evp-1",
                "builder",
                serde_json::json!({}),
                now,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "v-1",
                LedgerKind::VerdictIssued,
                "vgv-1",
                "judge.v1",
                serde_json::json!({"decision": "allow_merge"}),
                now,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "v-2",
                LedgerKind::VerdictIssued,
                "vgv-2",
                "judge.v1",
                serde_json::json!({"decision": "reject"}),
                now,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "rb-nw",
                LedgerKind::RollbackInitiated,
                "rel-X",
                "reviewer-nightwatch.v1",
                serde_json::json!({"reason": "p99 regression"}),
                now,
            ))
            .await
            .unwrap();

        let snap = collect(&ledger, &bell, now).await.unwrap();
        let out = render_prometheus(&snap);

        // Spot checks for the format contract.
        assert!(out.contains("# HELP"));
        assert!(out.contains("# TYPE"));
        assert!(out.contains("# TYPE jeryu_autonomous_promotion_total counter"));
        assert!(out.contains("# TYPE jeryu_kill_bell_state gauge"));
        assert!(out.contains("# TYPE jeryu_promotion_to_rollback_seconds histogram"));
        // Labels must be quoted; numbers unquoted.
        assert!(out.contains("jeryu_verdict_issued_total{decision=\"allow_merge\"} 1"));
        assert!(out.contains("jeryu_verdict_issued_total{decision=\"reject\"} 1"));
        // Nightwatch attribution: actor prefix path.
        assert!(out.contains("jeryu_nightwatch_rollback_total 1"));
        // Kill bell engaged via the real API.
        assert!(out.contains("jeryu_kill_bell_state 1"));
        // Histogram surfaces buckets + _sum + _count even when empty.
        assert!(out.contains("jeryu_promotion_to_rollback_seconds_bucket{le=\"+Inf\"}"));
        assert!(out.contains("jeryu_promotion_to_rollback_seconds_count"));

        // Critical invariant: exactly one HELP+TYPE per metric name.
        for name in [
            "jeryu_autonomous_promotion_total",
            "jeryu_nightwatch_rollback_total",
            "jeryu_kill_bell_engaged_total",
            "jeryu_kill_bell_resumed_total",
            "jeryu_verdict_issued_total",
            "jeryu_human_escalation_total",
            "jeryu_evidence_pack_created_total",
            "jeryu_merge_passport_invalidated_total",
            "jeryu_kill_bell_state",
            "jeryu_ledger_entries_last_24h",
            "jeryu_mean_time_to_rollback_seconds",
            "jeryu_promotion_to_rollback_seconds",
        ] {
            let help_count = out
                .lines()
                .filter(|l| l.starts_with(&format!("# HELP {} ", name)))
                .count();
            let type_count = out
                .lines()
                .filter(|l| l.starts_with(&format!("# TYPE {} ", name)))
                .count();
            assert_eq!(help_count, 1, "metric {name} must have exactly one HELP");
            assert_eq!(type_count, 1, "metric {name} must have exactly one TYPE");
        }
    }

    #[tokio::test]
    async fn nightwatch_attribution_matches_actor_or_reason() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let now = Utc::now();

        // 1) Actor prefix path.
        ledger
            .append(&make_entry(
                "rb-1",
                LedgerKind::RollbackInitiated,
                "rel-1",
                "reviewer-nightwatch.scheduler.v1",
                serde_json::json!({"reason": "manual"}),
                now,
            ))
            .await
            .unwrap();
        // 2) Reason substring path (actor is not nightwatch).
        ledger
            .append(&make_entry(
                "rb-2",
                LedgerKind::RollbackInitiated,
                "rel-2",
                "operator.alice",
                serde_json::json!({"reason": "nightwatch detected regression"}),
                now,
            ))
            .await
            .unwrap();
        // 3) Unrelated rollback — must not be counted.
        ledger
            .append(&make_entry(
                "rb-3",
                LedgerKind::RollbackInitiated,
                "rel-3",
                "operator.bob",
                serde_json::json!({"reason": "user request"}),
                now,
            ))
            .await
            .unwrap();

        let snap = collect(&ledger, &bell, now).await.unwrap();
        let nw = snap
            .counters
            .iter()
            .find(|c| c.name == "jeryu_nightwatch_rollback_total")
            .expect("nightwatch counter missing");
        assert_eq!(
            nw.value, 2,
            "should count both actor-prefix and reason-substring"
        );
    }

    #[tokio::test]
    async fn ledger_entries_last_24h_excludes_older_rows() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let now = Utc::now();
        ledger
            .append(&make_entry(
                "recent",
                LedgerKind::EvidencePackCreated,
                "evp-1",
                "a",
                serde_json::json!({}),
                now - Duration::hours(1),
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "stale",
                LedgerKind::EvidencePackCreated,
                "evp-2",
                "a",
                serde_json::json!({}),
                now - Duration::hours(48),
            ))
            .await
            .unwrap();
        let snap = collect(&ledger, &bell, now).await.unwrap();
        let g = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_ledger_entries_last_24h")
            .expect("24h gauge missing");
        assert_eq!(g.value, 1.0);
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Unmatched promotion/rollback ledger rows must NOT contribute to MTTR.
    /// A rollback for a subject that has never promoted, and a clock-skewed
    /// pair (rollback BEFORE promotion) must both be edge-cased cleanly:
    /// the lone rollback is ignored and a negative delta is clamped to 0.
    #[tokio::test]
    async fn mttr_pair_edge_cases_lone_rollback_and_clock_skew() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let now = Utc::now();
        // Lone rollback — no prior promotion for rel-X.
        ledger
            .append(&make_entry(
                "rb-orphan",
                LedgerKind::RollbackInitiated,
                "rel-X",
                "op",
                serde_json::json!({}),
                now,
            ))
            .await
            .unwrap();
        // Clock skew: rollback recorded BEFORE the promotion. Pair forms
        // because the ledger is ordered by recorded_at ASC; the rollback
        // hits first, finds no promo, is dropped. So this also adds zero.
        ledger
            .append(&make_entry(
                "rb-pre",
                LedgerKind::RollbackInitiated,
                "rel-Y",
                "op",
                serde_json::json!({}),
                now,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "promote-late",
                LedgerKind::DeploymentPromoted,
                "rel-Y",
                "op",
                serde_json::json!({}),
                now + Duration::seconds(5),
            ))
            .await
            .unwrap();
        let snap = collect(&ledger, &bell, now + Duration::seconds(60))
            .await
            .unwrap();
        let mttr = snap
            .gauges
            .iter()
            .find(|g| g.name == "jeryu_mean_time_to_rollback_seconds")
            .expect("mttr gauge missing");
        assert_eq!(mttr.value, 0.0, "no valid pairs → mttr stays 0");
        let h = snap
            .histograms
            .iter()
            .find(|h| h.name == "jeryu_promotion_to_rollback_seconds")
            .expect("histogram missing");
        assert_eq!(h.count, 0);
        assert_eq!(h.sum, 0.0);
    }

    /// Two distinct decision label values must produce exactly two
    /// `jeryu_verdict_issued_total{decision=...}` series — no merge, no
    /// duplication. Bounds the cardinality at the number of *observed*
    /// decision strings rather than letting an unbounded label set leak
    /// through (Prometheus protection).
    #[tokio::test]
    async fn decision_label_cardinality_bounded_by_observed_values() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let now = Utc::now();
        for (i, dec) in ["allow_merge", "reject", "allow_merge", "require_human"]
            .iter()
            .enumerate()
        {
            ledger
                .append(&make_entry(
                    &format!("v-{i}"),
                    LedgerKind::VerdictIssued,
                    &format!("vgv-{i}"),
                    "judge.v1",
                    serde_json::json!({"decision": dec}),
                    now,
                ))
                .await
                .unwrap();
        }
        let snap = collect(&ledger, &bell, now).await.unwrap();
        let verdict_series: Vec<&Counter> = snap
            .counters
            .iter()
            .filter(|c| c.name == "jeryu_verdict_issued_total")
            .collect();
        assert_eq!(
            verdict_series.len(),
            3,
            "exactly one series per distinct decision label; got {verdict_series:?}"
        );
        let labels: std::collections::HashSet<&str> = verdict_series
            .iter()
            .flat_map(|c| c.labels.iter().map(|(_, v)| v.as_str()))
            .collect();
        assert_eq!(
            labels,
            ["allow_merge", "reject", "require_human"]
                .iter()
                .copied()
                .collect()
        );
    }

    /// Each histogram bucket must be cumulative AND honor exact-boundary
    /// inclusion: a 60-second sample falls in the le=60 bucket; a 60.5s
    /// sample does not.
    #[tokio::test]
    async fn histogram_bucket_boundaries_are_inclusive_lower_exclusive_higher() {
        let pool = fresh_db().await;
        let ledger = SqlLedger::new(pool.clone());
        let bell = KillBell::new(pool);
        let base = Utc::now();
        // Exactly 60s pair: must land in le=60.
        ledger
            .append(&make_entry(
                "p60",
                LedgerKind::DeploymentPromoted,
                "rel-60",
                "op",
                serde_json::json!({}),
                base,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "r60",
                LedgerKind::RollbackInitiated,
                "rel-60",
                "op",
                serde_json::json!({}),
                base + Duration::seconds(60),
            ))
            .await
            .unwrap();
        // 61s pair: falls past le=60 but into le=300.
        ledger
            .append(&make_entry(
                "p61",
                LedgerKind::DeploymentPromoted,
                "rel-61",
                "op",
                serde_json::json!({}),
                base,
            ))
            .await
            .unwrap();
        ledger
            .append(&make_entry(
                "r61",
                LedgerKind::RollbackInitiated,
                "rel-61",
                "op",
                serde_json::json!({}),
                base + Duration::seconds(61),
            ))
            .await
            .unwrap();
        let snap = collect(&ledger, &bell, base + Duration::seconds(120))
            .await
            .unwrap();
        let h = snap
            .histograms
            .iter()
            .find(|h| h.name == "jeryu_promotion_to_rollback_seconds")
            .expect("histogram missing");
        let le_60 = h
            .buckets
            .iter()
            .find(|(b, _)| *b == 60.0)
            .map(|(_, c)| *c)
            .expect("le=60 bucket present");
        assert_eq!(le_60, 1, "exactly the 60s sample falls in le=60");
        let le_300 = h
            .buckets
            .iter()
            .find(|(b, _)| *b == 300.0)
            .map(|(_, c)| *c)
            .expect("le=300 bucket present");
        assert_eq!(le_300, 2, "both samples (60 and 61) fall in le=300");
        assert_eq!(h.count, 2);
        assert_eq!(h.sum, 121.0);
    }
}
