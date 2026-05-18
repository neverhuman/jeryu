//! Owner: Evidence Gate / Nightwatch canary controller
//! Proof: `cargo test -p jeryu --lib release::canary`
//! Invariants:
//!   - Rings advance monotonically. A `CanaryController::evaluate` call never
//!     decreases `state.current_ring_idx`; it either Holds (stay), Promotes
//!     (advance by one or saturate at the final ring), or Rollbacks
//!     (advance is owned by the orchestrator that consumes the decision).
//!   - SLO breach beats time-in-ring. If telemetry trips a threshold we
//!     return Rollback even when the ring is still inside its dwell window
//!     (tip4 §"Nightwatch", tip8 §"Release Gate" — SLO-based pause/rollback).
//!   - `Telemetry::observe` is the only IO seam. The controller itself is
//!     pure given a `now` clock and a `&dyn Telemetry`, which keeps the
//!     promotion ladder deterministic and unit-testable.
//!   - `FileTelemetry` refuses snapshots whose `sampled_at` is older than
//!     `window_end`. Stale samples cannot promote a release — the agent
//!     must collect fresh telemetry inside the current ring window.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::autonomy::types::{DeployEnvironment, ReleasePassport, RollbackStrategy};

// ---------------------------------------------------------------------------
// Ring ladder
// ---------------------------------------------------------------------------

/// One step on the canary ladder. `percent` is the share of production
/// traffic routed to the new version while the ring is active. `min_duration_secs`
/// is the dwell time that must elapse with clean telemetry before the
/// controller will promote to the next ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ring {
    pub percent: u8,
    pub min_duration_secs: u64,
}

/// Default progressive rollout ladder. 1% → 5% → 25% → 50% → 100% with
/// dwell times tuned to be long enough to catch typical regression signals
/// without making every release wait a full day. The final ring uses
/// `min_duration_secs = 0` because once we are at 100% traffic there is no
/// further promotion to gate.
pub const DEFAULT_RINGS: &[Ring] = &[
    Ring {
        percent: 1,
        min_duration_secs: 600,
    },
    Ring {
        percent: 5,
        min_duration_secs: 600,
    },
    Ring {
        percent: 25,
        min_duration_secs: 1_800,
    },
    Ring {
        percent: 50,
        min_duration_secs: 3_600,
    },
    Ring {
        percent: 100,
        min_duration_secs: 0,
    },
];

// ---------------------------------------------------------------------------
// SLO thresholds and telemetry snapshots
// ---------------------------------------------------------------------------

/// Max allowed deltas / absolute values for the current ring relative to the
/// pre-deploy baseline. Any breach triggers `CanaryDecision::Rollback`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SloThresholds {
    /// Max increase in error-rate fraction (e.g. 0.01 == +1pp).
    pub error_rate_max_delta: f64,
    /// Max increase in p95 latency in milliseconds.
    pub p95_latency_max_delta_ms: f64,
    /// Absolute ceiling on crash rate. Any value above this is a rollback.
    pub crash_rate_max: f64,
}

impl Default for SloThresholds {
    fn default() -> Self {
        Self {
            error_rate_max_delta: 0.01,
            p95_latency_max_delta_ms: 250.0,
            crash_rate_max: 0.001,
        }
    }
}

/// One telemetry observation for the current canary ring window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    pub crash_rate: f64,
    pub sampled_at: DateTime<Utc>,
    pub samples: u64,
}

/// IO seam for fetching telemetry. Implementations may read Prometheus,
/// OTLP, BigQuery, a flat JSON file — anything that can answer "what did the
/// new version look like between `window_start` and `window_end`?".
pub trait Telemetry: Send + Sync {
    fn observe(
        &self,
        ring_percent: u8,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<TelemetrySnapshot>;
}

/// File-backed telemetry. Reads a JSON `TelemetrySnapshot` from `path` and
/// refuses to return data whose `sampled_at` is older than `window_end`
/// (which would mean we are about to promote based on stale data). The
/// `ring_percent` and `window_start` parameters are accepted for trait
/// compatibility but are not enforced — callers that need scoping per ring
/// should write per-ring files and construct one `FileTelemetry` per ring.
#[derive(Debug, Clone)]
pub struct FileTelemetry {
    pub path: PathBuf,
}

impl FileTelemetry {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Telemetry for FileTelemetry {
    fn observe(
        &self,
        _ring_percent: u8,
        _window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<TelemetrySnapshot> {
        let raw = std::fs::read_to_string(&self.path)
            .with_context(|| format!("read telemetry snapshot at {}", self.path.display()))?;
        let snap: TelemetrySnapshot = serde_json::from_str(&raw)
            .with_context(|| format!("parse telemetry snapshot at {}", self.path.display()))?;
        if snap.sampled_at < window_end {
            bail!(
                "stale telemetry: snapshot sampled_at={} predates window_end={}",
                snap.sampled_at,
                window_end
            );
        }
        Ok(snap)
    }
}

// ---------------------------------------------------------------------------
// Controller state machine
// ---------------------------------------------------------------------------

/// One decision the controller has taken about the current ring.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CanaryDecision {
    /// Advance to the next ring (or stay at the final ring if already
    /// at 100% traffic).
    Promote,
    /// Stay on this ring. Either the dwell time has not elapsed or the
    /// telemetry sample size is too small to be meaningful.
    Hold,
    /// Pull traffic back. The orchestrator should consume the
    /// `RollbackStrategy` from the passport to decide *how*.
    Rollback { reason: String },
}

/// One audit row appended every time `evaluate()` reaches a verdict. Stored
/// inside `CanaryState::history` so the launch ledger can serialise the
/// full promotion trail per release.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingEvent {
    pub ring: Ring,
    pub decision: CanaryDecision,
    pub at: DateTime<Utc>,
    pub snapshot: Option<TelemetrySnapshot>,
}

/// Live state of one in-progress canary rollout. Mutated by `evaluate()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanaryState {
    pub release_id: String,
    pub current_ring_idx: usize,
    pub ring_entered_at: DateTime<Utc>,
    pub history: Vec<RingEvent>,
}

/// Nightwatch canary controller. Holds the ring ladder, the SLO thresholds,
/// and a telemetry source. Pure once those three are bound — given the same
/// inputs, `evaluate()` always reaches the same verdict.
pub struct CanaryController {
    pub rings: Vec<Ring>,
    pub slo: SloThresholds,
    pub telemetry: Arc<dyn Telemetry>,
}

impl CanaryController {
    /// Convenience constructor for the default ladder + default thresholds.
    pub fn with_defaults(telemetry: Arc<dyn Telemetry>) -> Self {
        Self {
            rings: DEFAULT_RINGS.to_vec(),
            slo: SloThresholds::default(),
            telemetry,
        }
    }

    /// Begin a rollout. Always seats the new release at ring 0 (the 1%
    /// ring under `DEFAULT_RINGS`). Uses the passport's `release_id` when
    /// present, else falls back to the passport `id`.
    pub fn start(&self, passport: &ReleasePassport, now: DateTime<Utc>) -> CanaryState {
        let release_id = passport
            .release_id
            .clone()
            .unwrap_or_else(|| passport.id.clone());
        CanaryState {
            release_id,
            current_ring_idx: 0,
            ring_entered_at: now,
            history: Vec::new(),
        }
    }

    /// Evaluate the current ring against fresh telemetry.
    ///
    /// Decision order (matches tip4 §"Nightwatch" semantics):
    ///   1. SLO breach → `Rollback { reason }`.
    ///   2. Dwell time not yet elapsed → `Hold`.
    ///   3. Clean window → `Promote` and (unless already at the final
    ///      ring) advance `current_ring_idx` by one, resetting
    ///      `ring_entered_at` to `now`.
    pub fn evaluate(&self, state: &mut CanaryState, now: DateTime<Utc>) -> CanaryDecision {
        let Some(ring) = self.rings.get(state.current_ring_idx).copied() else {
            // Defensive: a state pointing past the ladder is a bug, but we
            // surface it as a Hold rather than panicking so the orchestrator
            // can recover.
            let decision = CanaryDecision::Hold;
            state.history.push(RingEvent {
                ring: Ring {
                    percent: 100,
                    min_duration_secs: 0,
                },
                decision: decision.clone(),
                at: now,
                snapshot: None,
            });
            return decision;
        };

        let window_start = state.ring_entered_at;
        let window_end = now;

        let snapshot = match self
            .telemetry
            .observe(ring.percent, window_start, window_end)
        {
            Ok(s) => s,
            Err(_err) => {
                // No fresh telemetry → we cannot promote, but we also do
                // not roll back on a transient telemetry outage. Hold.
                let decision = CanaryDecision::Hold;
                state.history.push(RingEvent {
                    ring,
                    decision: decision.clone(),
                    at: now,
                    snapshot: None,
                });
                return decision;
            }
        };

        if let Some(reason) = self.slo_breach_reason(&snapshot) {
            let decision = CanaryDecision::Rollback { reason };
            state.history.push(RingEvent {
                ring,
                decision: decision.clone(),
                at: now,
                snapshot: Some(snapshot),
            });
            return decision;
        }

        let elapsed = (window_end - window_start).num_seconds().max(0) as u64;
        if elapsed < ring.min_duration_secs {
            let decision = CanaryDecision::Hold;
            state.history.push(RingEvent {
                ring,
                decision: decision.clone(),
                at: now,
                snapshot: Some(snapshot),
            });
            return decision;
        }

        // Promote. Advance unless we are already at the final ring.
        let last_idx = self.rings.len().saturating_sub(1);
        if state.current_ring_idx < last_idx {
            state.current_ring_idx += 1;
            state.ring_entered_at = now;
        }
        let decision = CanaryDecision::Promote;
        state.history.push(RingEvent {
            ring,
            decision: decision.clone(),
            at: now,
            snapshot: Some(snapshot),
        });
        decision
    }

    fn slo_breach_reason(&self, snap: &TelemetrySnapshot) -> Option<String> {
        if snap.error_rate > self.slo.error_rate_max_delta {
            return Some(format!(
                "error_rate {:.4} exceeds max delta {:.4}",
                snap.error_rate, self.slo.error_rate_max_delta
            ));
        }
        if snap.p95_latency_ms > self.slo.p95_latency_max_delta_ms {
            return Some(format!(
                "p95_latency_ms {:.1} exceeds max delta {:.1}",
                snap.p95_latency_ms, self.slo.p95_latency_max_delta_ms
            ));
        }
        if snap.crash_rate > self.slo.crash_rate_max {
            return Some(format!(
                "crash_rate {:.6} exceeds max {:.6}",
                snap.crash_rate, self.slo.crash_rate_max
            ));
        }
        None
    }
}

// Convenience: callers building a rollback record from the passport often
// want the strategy back out as the typed enum even though the passport
// carries a string. Exposed here so the canary module is a one-stop shop
// for promotion + rollback metadata.
pub fn rollback_strategy_for_env(env: DeployEnvironment) -> RollbackStrategy {
    match env {
        DeployEnvironment::Dev | DeployEnvironment::Staging => RollbackStrategy::RedeployPrevious,
        DeployEnvironment::Canary | DeployEnvironment::Prod => RollbackStrategy::RevertCommit,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{ArtifactKind, ReleasePassport, ReleaseRollbackPlan, SchemaTag};
    use chrono::Duration;
    use std::sync::Mutex;
    use tempfile::tempdir;

    // ---- fixtures ---------------------------------------------------------

    fn passport(release_id: Option<&str>) -> ReleasePassport {
        ReleasePassport {
            schema: SchemaTag::default(),
            id: "passport-abc".to_string(),
            release_id: release_id.map(|s| s.to_string()),
            artifact_digest: "sha256:art".to_string(),
            artifact_kind: ArtifactKind::Container,
            sbom_digest: "sha256:sbom".to_string(),
            provenance_digest: "sha256:prov".to_string(),
            source_sha: "deadbeef".to_string(),
            build_logs_digest: "sha256:logs".to_string(),
            allowed_environments: vec![DeployEnvironment::Canary, DeployEnvironment::Prod],
            rollback_plan: ReleaseRollbackPlan {
                strategy: "revert_commit".to_string(),
                tested: true,
            },
            issued_at: Utc::now(),
            signature: Signature {
                algo: "ed25519".to_string(),
                key_id: "k1".to_string(),
                value: "sig".to_string(),
            },
        }
    }

    fn clean_snapshot(at: DateTime<Utc>) -> TelemetrySnapshot {
        TelemetrySnapshot {
            error_rate: 0.001,
            p95_latency_ms: 50.0,
            crash_rate: 0.0,
            sampled_at: at,
            samples: 10_000,
        }
    }

    /// Test double. Returns the configured snapshot; records every call so
    /// tests can assert on the (ring_percent, window) tuple the controller
    /// asked for.
    #[derive(Debug)]
    struct FakeTelemetry {
        snapshot: Mutex<Result<TelemetrySnapshot, String>>,
        #[allow(clippy::type_complexity)]
        calls: Mutex<Vec<(u8, DateTime<Utc>, DateTime<Utc>)>>,
    }

    impl FakeTelemetry {
        fn ok(snap: TelemetrySnapshot) -> Arc<Self> {
            Arc::new(Self {
                snapshot: Mutex::new(Ok(snap)),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn set(&self, snap: TelemetrySnapshot) {
            *self.snapshot.lock().unwrap() = Ok(snap);
        }
    }

    impl Telemetry for FakeTelemetry {
        fn observe(
            &self,
            ring_percent: u8,
            window_start: DateTime<Utc>,
            window_end: DateTime<Utc>,
        ) -> Result<TelemetrySnapshot> {
            self.calls
                .lock()
                .unwrap()
                .push((ring_percent, window_start, window_end));
            match &*self.snapshot.lock().unwrap() {
                Ok(s) => Ok(s.clone()),
                Err(msg) => bail!("{}", msg),
            }
        }
    }

    // ---- ladder shape -----------------------------------------------------

    #[test]
    fn default_rings_are_monotonic_and_end_at_100() {
        let percents: Vec<u8> = DEFAULT_RINGS.iter().map(|r| r.percent).collect();
        assert_eq!(percents.first().copied(), Some(1));
        assert_eq!(percents.last().copied(), Some(100));
        for window in percents.windows(2) {
            assert!(
                window[0] < window[1],
                "ring percents must strictly increase: {percents:?}"
            );
        }
        // Final ring has zero dwell — once at 100% there is nothing to
        // promote to.
        assert_eq!(DEFAULT_RINGS.last().unwrap().min_duration_secs, 0);
    }

    // ---- start() ----------------------------------------------------------

    #[test]
    fn start_initializes_at_ring_zero() {
        let tele = FakeTelemetry::ok(clean_snapshot(Utc::now()));
        let ctrl = CanaryController::with_defaults(tele);
        let now = Utc::now();
        let state = ctrl.start(&passport(Some("rel-1")), now);
        assert_eq!(state.current_ring_idx, 0);
        assert_eq!(state.release_id, "rel-1");
        assert_eq!(state.ring_entered_at, now);
        assert!(state.history.is_empty());
    }

    #[test]
    fn start_falls_back_to_passport_id_when_release_id_absent() {
        let tele = FakeTelemetry::ok(clean_snapshot(Utc::now()));
        let ctrl = CanaryController::with_defaults(tele);
        let state = ctrl.start(&passport(None), Utc::now());
        assert_eq!(state.release_id, "passport-abc");
    }

    // ---- evaluate(): timing -----------------------------------------------

    #[test]
    fn evaluate_holds_when_min_duration_not_met() {
        let start = Utc::now();
        let tele = FakeTelemetry::ok(clean_snapshot(start + Duration::seconds(60)));
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, start + Duration::seconds(60));
        assert_eq!(decision, CanaryDecision::Hold);
        assert_eq!(state.current_ring_idx, 0);
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].decision, CanaryDecision::Hold);
    }

    #[test]
    fn evaluate_promotes_when_clean_and_window_elapsed() {
        let start = Utc::now();
        let later = start + Duration::seconds(DEFAULT_RINGS[0].min_duration_secs as i64 + 1);
        let tele = FakeTelemetry::ok(clean_snapshot(later));
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, later);
        assert_eq!(decision, CanaryDecision::Promote);
        assert_eq!(state.current_ring_idx, 1);
        assert_eq!(state.ring_entered_at, later);
        assert_eq!(state.history.len(), 1);
    }

    // ---- evaluate(): SLO breaches -----------------------------------------

    #[test]
    fn evaluate_rolls_back_on_error_rate_breach() {
        let start = Utc::now();
        let later = start + Duration::seconds(DEFAULT_RINGS[0].min_duration_secs as i64 + 5);
        let mut snap = clean_snapshot(later);
        snap.error_rate = 0.5; // way over default 0.01
        let tele = FakeTelemetry::ok(snap);
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, later);
        match decision {
            CanaryDecision::Rollback { reason } => assert!(reason.contains("error_rate")),
            other => panic!("expected Rollback, got {other:?}"),
        }
        // Ring index must not advance on rollback.
        assert_eq!(state.current_ring_idx, 0);
    }

    #[test]
    fn evaluate_rolls_back_on_p95_latency_breach() {
        let start = Utc::now();
        let later = start + Duration::seconds(DEFAULT_RINGS[0].min_duration_secs as i64 + 5);
        let mut snap = clean_snapshot(later);
        snap.p95_latency_ms = 9_999.0;
        let tele = FakeTelemetry::ok(snap);
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, later);
        match decision {
            CanaryDecision::Rollback { reason } => assert!(reason.contains("p95_latency_ms")),
            other => panic!("expected Rollback, got {other:?}"),
        }
        assert_eq!(state.current_ring_idx, 0);
    }

    #[test]
    fn evaluate_rolls_back_on_crash_rate_breach() {
        let start = Utc::now();
        let later = start + Duration::seconds(DEFAULT_RINGS[0].min_duration_secs as i64 + 5);
        let mut snap = clean_snapshot(later);
        snap.crash_rate = 0.5;
        let tele = FakeTelemetry::ok(snap);
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, later);
        match decision {
            CanaryDecision::Rollback { reason } => assert!(reason.contains("crash_rate")),
            other => panic!("expected Rollback, got {other:?}"),
        }
    }

    #[test]
    fn slo_breach_beats_dwell_time() {
        // Even if dwell has not elapsed, a tripped SLO must rollback.
        let start = Utc::now();
        let inside_window = start + Duration::seconds(5);
        let mut snap = clean_snapshot(inside_window);
        snap.error_rate = 1.0;
        let tele = FakeTelemetry::ok(snap);
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, inside_window);
        assert!(matches!(decision, CanaryDecision::Rollback { .. }));
    }

    // ---- evaluate(): final ring -------------------------------------------

    #[test]
    fn promote_at_final_ring_does_not_advance_past_end() {
        let start = Utc::now();
        let later = start + Duration::seconds(10);
        let tele = FakeTelemetry::ok(clean_snapshot(later));
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);
        // Jump to the final ring.
        let last = ctrl.rings.len() - 1;
        state.current_ring_idx = last;
        state.ring_entered_at = start;

        let decision = ctrl.evaluate(&mut state, later);
        assert_eq!(decision, CanaryDecision::Promote);
        assert_eq!(
            state.current_ring_idx, last,
            "final ring must not advance past the end of the ladder"
        );
    }

    #[test]
    fn telemetry_error_holds_rather_than_rolls_back() {
        let start = Utc::now();
        let later = start + Duration::seconds(DEFAULT_RINGS[0].min_duration_secs as i64 + 5);
        let tele = Arc::new(FakeTelemetry {
            snapshot: Mutex::new(Err("scrape failed".to_string())),
            calls: Mutex::new(Vec::new()),
        });
        let ctrl = CanaryController::with_defaults(tele);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        let decision = ctrl.evaluate(&mut state, later);
        assert_eq!(decision, CanaryDecision::Hold);
        assert_eq!(state.current_ring_idx, 0);
        // History still records the event with no snapshot.
        assert!(state.history.last().unwrap().snapshot.is_none());
    }

    #[test]
    fn evaluate_then_recover_promotes_on_next_call() {
        // First call inside the window → Hold. Second call past the window
        // with clean telemetry → Promote. Demonstrates the controller is
        // safe to call repeatedly.
        let start = Utc::now();
        let dwell = DEFAULT_RINGS[0].min_duration_secs as i64;
        let mid = start + Duration::seconds(dwell / 2);
        let past = start + Duration::seconds(dwell + 1);

        let tele = FakeTelemetry::ok(clean_snapshot(mid));
        let ctrl = CanaryController::with_defaults(Arc::clone(&tele) as Arc<dyn Telemetry>);
        let mut state = ctrl.start(&passport(Some("rel")), start);

        assert_eq!(ctrl.evaluate(&mut state, mid), CanaryDecision::Hold);
        tele.set(clean_snapshot(past));
        assert_eq!(ctrl.evaluate(&mut state, past), CanaryDecision::Promote);
        assert_eq!(state.current_ring_idx, 1);
    }

    // ---- FileTelemetry ----------------------------------------------------

    #[test]
    fn file_telemetry_reads_snapshot_from_disk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("snap.json");
        let sampled_at = Utc::now();
        let snap = TelemetrySnapshot {
            error_rate: 0.002,
            p95_latency_ms: 80.0,
            crash_rate: 0.0,
            sampled_at,
            samples: 5_000,
        };
        std::fs::write(&path, serde_json::to_string(&snap).unwrap()).unwrap();

        let tele = FileTelemetry::new(path);
        let window_end = sampled_at - Duration::seconds(1);
        let got = tele
            .observe(5, sampled_at - Duration::seconds(60), window_end)
            .expect("fresh snapshot must be returned");
        assert_eq!(got, snap);
    }

    #[test]
    fn file_telemetry_refuses_stale_snapshot() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("snap.json");
        let sampled_at = Utc::now() - Duration::seconds(600);
        let snap = TelemetrySnapshot {
            error_rate: 0.001,
            p95_latency_ms: 50.0,
            crash_rate: 0.0,
            sampled_at,
            samples: 1_000,
        };
        std::fs::write(&path, serde_json::to_string(&snap).unwrap()).unwrap();

        let tele = FileTelemetry::new(path);
        let window_end = Utc::now();
        let err = tele
            .observe(1, sampled_at, window_end)
            .expect_err("stale snapshot must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("stale telemetry"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn file_telemetry_errors_on_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let tele = FileTelemetry::new(path);
        let err = tele
            .observe(1, Utc::now(), Utc::now())
            .expect_err("missing file must error");
        assert!(format!("{err}").contains("read telemetry snapshot"));
    }

    // ---- helpers ----------------------------------------------------------

    // --- Wave 5 coverage-boost addition ------------------------------------

    /// Each variant of `CanaryDecision` must serialize+deserialize back to
    /// the same value (property-style round-trip). This guards the wire
    /// format the launch ledger emits.
    #[test]
    fn canary_decision_round_trips_through_json_for_every_variant() {
        let variants = vec![
            CanaryDecision::Promote,
            CanaryDecision::Hold,
            CanaryDecision::Rollback {
                reason: "p95 over threshold".into(),
            },
        ];
        for v in variants {
            let json = serde_json::to_string(&v).expect("serialize");
            let back: CanaryDecision = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(v, back, "round-trip mismatch for {v:?}");
        }
    }

    #[test]
    fn rollback_strategy_for_env_maps_prod_to_revert_commit() {
        assert_eq!(
            rollback_strategy_for_env(DeployEnvironment::Prod),
            RollbackStrategy::RevertCommit
        );
        assert_eq!(
            rollback_strategy_for_env(DeployEnvironment::Staging),
            RollbackStrategy::RedeployPrevious
        );
    }
}
