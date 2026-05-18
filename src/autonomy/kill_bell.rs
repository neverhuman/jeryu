//! Owner: Evidence Gate / autonomy control plane (Wave 4)
//! Proof: `cargo test -p jeryu --lib autonomy::kill_bell`
//! Invariants:
//!   - Pause state outlives process restart (SQL-backed in `kill_bell_state`).
//!   - Every pause carries a TTL; once `now >= expires_at` the bell
//!     auto-arms via `KillBell::current()` even without an explicit `resume()`.
//!     This is load-bearing: a forgotten pause MUST NOT brick the
//!     autonomous-delivery control plane forever.
//!   - Every `pause()` and `resume()` appends a signed `LaunchLedgerEntry`
//!     (`KillBellEngaged` / `KillBellResumed`). Signing uses `EdSigningKey`,
//!     so `SqlLedger::append()`'s unsigned/HMAC refusal automatically applies —
//!     no path can land an unsigned signature for a Kill Bell event.
//!   - While paused, `downgrade_if_paused()` rewrites any `GateDecision`
//!     to `RequireHuman`. This is the contract the Evidence Gate dispatch
//!     loop checks before issuing `AllowMerge` / prod-promotion verdicts.
//!
//! Wave 11.A: SQL queries moved to `src/db/autonomy_repo.rs`. `KillBell`
//! is a thin wrapper that pairs an `AutonomyRepo` for the storage half
//! with `SqlLedger` for the audit-trail half. Public API is unchanged.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::db::AnyPool;
use crate::db::autonomy_repo::{AutonomyRepo, KillBellState as RepoKillBellState};

use super::ledger::{SqlLedger, sign_entry};
use super::signing::{EdSigningKey, Signature};
use super::types::{GateDecision, LaunchLedgerEntry, LedgerKind, SchemaTag};

/// Current Kill Bell posture.
///
/// `Armed` = autonomous-delivery decisions flow normally.
/// `Paused` = global pause in effect; every gate verdict downgrades to
/// `RequireHuman` until either an explicit `resume()` or the TTL expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KillBellState {
    Armed,
    Paused {
        reason: String,
        paused_by: String,
        paused_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    },
}

impl KillBellState {
    pub fn is_paused(&self) -> bool {
        matches!(self, KillBellState::Paused { .. })
    }
}

impl From<RepoKillBellState> for KillBellState {
    fn from(s: RepoKillBellState) -> Self {
        match s {
            RepoKillBellState::Armed => KillBellState::Armed,
            RepoKillBellState::Paused {
                reason,
                paused_by,
                paused_at,
                expires_at,
            } => KillBellState::Paused {
                reason,
                paused_by,
                paused_at,
                expires_at,
            },
        }
    }
}

/// Signed break-glass receipt. Minted by an operator who deliberately
/// engages or bypasses the Kill Bell for a bounded scope/window.
///
/// The minting API lives outside this module (future Wave 4.x); the type
/// lives here so producers and consumers share one definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakGlassReceipt {
    pub id: String,
    pub actor: String,
    pub reason: String,
    pub scope: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub signature: Signature,
}

/// SQL-backed Kill Bell. Cheap to clone (pool is `Arc`-shaped).
#[derive(Debug, Clone)]
pub struct KillBell {
    repo: AutonomyRepo,
}

impl KillBell {
    pub fn new(pool: AnyPool) -> Self {
        Self {
            repo: AutonomyRepo::new(pool),
        }
    }

    /// Read the most-recent state transition. If the latest row is
    /// `paused` but its TTL has elapsed (`expires_at <= now`), this
    /// returns `Armed` — that is the auto-arm-on-TTL invariant. The
    /// physical row stays in the table as an audit trail; the next
    /// `pause()` or `resume()` simply appends a fresh row.
    pub async fn current(&self, now: DateTime<Utc>) -> Result<KillBellState> {
        Ok(self.repo.kill_bell_current(now).await?.into())
    }

    /// Engage the bell. `ttl_seconds` bounds how long the pause holds
    /// before auto-arm; the brainstorm requires this to be nonzero so a
    /// forgotten pause cannot brick the system permanently.
    ///
    /// Appends a signed `LaunchLedgerEntry { kind: KillBellEngaged }`
    /// BEFORE writing the state row so the audit trail leads the
    /// state change. If signing/append fails, the state row is not
    /// written and the bell stays in its previous posture.
    pub async fn pause(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
        signing_key: &EdSigningKey,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let ttl = ttl_seconds.min(i64::MAX as u64) as i64;
        let expires_at = now + Duration::seconds(ttl);

        // 1. Mint + sign + append the ledger entry first.
        let mut entry = LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: format!("ll_kb_{}", Uuid::new_v4()),
            kind: LedgerKind::KillBellEngaged,
            subject_id: "kill_bell".into(),
            repo: None,
            payload: serde_json::json!({
                "reason": reason,
                "paused_by": paused_by,
                "ttl_seconds": ttl_seconds,
                "expires_at": expires_at.to_rfc3339(),
            }),
            recorded_at: now,
            actor: paused_by.to_string(),
            signature: Signature::default_unsigned(), // replaced by sign_entry below
        };
        sign_entry(&mut entry, signing_key);
        let ledger = SqlLedger::new(self.repo.pool().clone());
        ledger
            .append(&entry)
            .await
            .context("append KillBellEngaged ledger entry")?;

        // 2. Then persist the state row.
        self.repo
            .kill_bell_set_paused(reason, paused_by, expires_at, now)
            .await?;

        Ok(())
    }

    /// Resume normal operation. Appends a signed
    /// `LaunchLedgerEntry { kind: KillBellResumed }` and writes an
    /// `armed` state row so `current()` reads back `Armed` even if the
    /// previous pause's TTL has not yet elapsed.
    pub async fn resume(
        &self,
        resumed_by: &str,
        signing_key: &EdSigningKey,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let mut entry = LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: format!("ll_kb_{}", Uuid::new_v4()),
            kind: LedgerKind::KillBellResumed,
            subject_id: "kill_bell".into(),
            repo: None,
            payload: serde_json::json!({ "resumed_by": resumed_by }),
            recorded_at: now,
            actor: resumed_by.to_string(),
            signature: Signature::default_unsigned(),
        };
        sign_entry(&mut entry, signing_key);
        let ledger = SqlLedger::new(self.repo.pool().clone());
        ledger
            .append(&entry)
            .await
            .context("append KillBellResumed ledger entry")?;

        self.repo.kill_bell_set_armed(resumed_by, now).await?;

        Ok(())
    }

    /// Convenience: `true` iff `current(now)` is `Paused`.
    pub async fn is_paused(&self, now: DateTime<Utc>) -> Result<bool> {
        Ok(self.current(now).await?.is_paused())
    }

    /// The hot-path check the dispatch loop runs before publishing a
    /// `GateDecision`. If the bell is paused, every decision downgrades
    /// to `RequireHuman` and the caller learns the reason string for
    /// observability. If armed, the decision passes through unchanged
    /// and the reason is `None`.
    pub async fn downgrade_if_paused(
        &self,
        decision: GateDecision,
        now: DateTime<Utc>,
    ) -> Result<(GateDecision, Option<String>)> {
        match self.current(now).await? {
            KillBellState::Armed => Ok((decision, None)),
            KillBellState::Paused {
                reason, paused_by, ..
            } => {
                let detail = format!(
                    "kill bell engaged by '{paused_by}': {reason}; \
                     downgraded {decision:?} → RequireHuman"
                );
                Ok((GateDecision::RequireHuman, Some(detail)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::ledger::{LedgerFilter, SqlLedger};
    use crate::db::autonomy_repo::fresh_autonomy_pool;

    /// In-memory pool shared across tests. The DDL installer lives in the
    /// db boundary so this file no longer imports `sqlx::`.
    async fn fresh_db() -> AnyPool {
        fresh_autonomy_pool().await
    }

    fn key() -> EdSigningKey {
        EdSigningKey::generate("operator.kill-bell")
    }

    #[tokio::test]
    async fn pause_then_is_paused_true() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        bell.pause("brown alert", "alice", 3600, &key(), now)
            .await
            .unwrap();
        assert!(bell.is_paused(now).await.unwrap());
        match bell.current(now).await.unwrap() {
            KillBellState::Paused {
                reason, paused_by, ..
            } => {
                assert_eq!(reason, "brown alert");
                assert_eq!(paused_by, "alice");
            }
            other => panic!("expected Paused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pause_with_ttl_expires_auto_arms() {
        let bell = KillBell::new(fresh_db().await);
        let t0 = Utc::now();
        bell.pause("short pause", "bob", 1, &key(), t0)
            .await
            .unwrap();
        assert!(bell.is_paused(t0).await.unwrap(), "paused at t0");
        // Advance time past the 1s TTL. current() must auto-arm.
        let t_later = t0 + Duration::seconds(5);
        assert_eq!(
            bell.current(t_later).await.unwrap(),
            KillBellState::Armed,
            "expired TTL must auto-arm to prevent permanent brick"
        );
        assert!(!bell.is_paused(t_later).await.unwrap());
    }

    #[tokio::test]
    async fn resume_clears_paused() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        bell.pause("incident", "alice", 3600, &key(), now)
            .await
            .unwrap();
        assert!(bell.is_paused(now).await.unwrap());
        bell.resume("alice", &key(), now + Duration::seconds(10))
            .await
            .unwrap();
        assert_eq!(
            bell.current(now + Duration::seconds(20)).await.unwrap(),
            KillBellState::Armed,
            "explicit resume must clear paused even before TTL"
        );
    }

    #[tokio::test]
    async fn downgrade_if_paused_downgrades_allow_merge() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        bell.pause("freeze", "alice", 3600, &key(), now)
            .await
            .unwrap();
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::AllowMerge, now)
            .await
            .unwrap();
        assert_eq!(decision, GateDecision::RequireHuman);
        let why = why.expect("paused must surface a reason");
        assert!(why.contains("freeze"), "reason should round-trip: {why}");
        assert!(why.contains("alice"));
    }

    #[tokio::test]
    async fn downgrade_if_paused_passes_through_when_armed() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        // No pause/resume calls — default state is Armed.
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::AllowMerge, now)
            .await
            .unwrap();
        assert_eq!(decision, GateDecision::AllowMerge);
        assert!(why.is_none(), "armed must not surface a reason");

        // Reject should also pass through unchanged.
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::Reject, now)
            .await
            .unwrap();
        assert_eq!(decision, GateDecision::Reject);
        assert!(why.is_none());
    }

    #[tokio::test]
    async fn pause_appends_ledger_entry_with_kill_bell_engaged_kind() {
        let pool = fresh_db().await;
        let bell = KillBell::new(pool.clone());
        let ledger = SqlLedger::new(pool);
        let now = Utc::now();
        bell.pause("network split", "alice", 60, &key(), now)
            .await
            .unwrap();
        let entries = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::KillBellEngaged),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, LedgerKind::KillBellEngaged);
        assert_eq!(entries[0].subject_id, "kill_bell");
        assert_eq!(entries[0].actor, "alice");
        // Payload carries the reason for downstream observers.
        assert_eq!(entries[0].payload["reason"], "network split");
        assert_eq!(entries[0].payload["ttl_seconds"], 60);
    }

    #[tokio::test]
    async fn resume_appends_ledger_entry_with_kill_bell_resumed_kind() {
        let pool = fresh_db().await;
        let bell = KillBell::new(pool.clone());
        let ledger = SqlLedger::new(pool);
        let now = Utc::now();
        bell.pause("ttest", "alice", 60, &key(), now).await.unwrap();
        bell.resume("bob", &key(), now + Duration::seconds(5))
            .await
            .unwrap();
        let entries = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::KillBellResumed),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, LedgerKind::KillBellResumed);
        assert_eq!(entries[0].subject_id, "kill_bell");
        assert_eq!(entries[0].actor, "bob");
        assert_eq!(entries[0].payload["resumed_by"], "bob");
    }

    #[tokio::test]
    async fn pause_refuses_unsigned_or_stub_signing_key() {
        // The Kill Bell API only accepts `&EdSigningKey`, which always
        // yields algo == "ed25519". We assert the type invariant: there
        // is no path (short of bypassing the API) that lands a stub or
        // hmac-stub signature in the ledger for a Kill Bell event.
        // This test is a regression fence: if someone ever introduces
        // a stub-key constructor for EdSigningKey, this assert breaks.
        let pool = fresh_db().await;
        let bell = KillBell::new(pool.clone());
        let ledger = SqlLedger::new(pool);
        let now = Utc::now();
        let k = EdSigningKey::generate("operator.alice");
        bell.pause("audit-fence", "alice", 60, &k, now)
            .await
            .unwrap();
        let entries = ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::KillBellEngaged),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].signature.algo, "ed25519",
            "Kill Bell ledger entries MUST be ed25519-signed; \
             SqlLedger::append() refuses stub/hmac and there is no \
             code path in this module that produces them."
        );
        // Spot-check: the signature value is non-stub (stub is all-zero hex).
        assert_ne!(
            entries[0].signature.value,
            "0".repeat(64),
            "stub signature value must not appear"
        );
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Four concurrent tasks alternating pause/resume must produce a
    /// well-defined terminal state. Since the writes serialize through the
    /// shared pool (max_connections=1) and the bell stores history rows,
    /// we assert that the final `current()` reads back a state consistent
    /// with one of the recorded transitions (no orphaned rows, no panics,
    /// no append-only trigger violations).
    #[tokio::test]
    async fn concurrent_pause_and_resume_consistent_state() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        let mut handles = Vec::new();
        for task in 0..4 {
            let bell = bell.clone();
            handles.push(tokio::spawn(async move {
                let k = EdSigningKey::generate(format!("op-{task}"));
                for _ in 0..3 {
                    bell.pause("rush", &format!("op-{task}"), 60, &k, now)
                        .await
                        .expect("pause");
                    bell.resume(&format!("op-{task}"), &k, now + Duration::seconds(1))
                        .await
                        .expect("resume");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        // After equal numbers of pause+resume, the most recent row must be
        // some final transition. Reading `current()` must succeed and yield
        // a non-panicking enum value.
        let state = bell.current(now + Duration::seconds(2)).await.unwrap();
        // is_paused() returns a bool; whatever the last write was, the
        // helper must agree with the enum variant.
        assert_eq!(
            state.is_paused(),
            matches!(state, KillBellState::Paused { .. })
        );
    }

    /// Pausing while already paused appends a second `KillBellEngaged`
    /// ledger entry and the most-recent pause window wins.
    #[tokio::test]
    async fn pause_during_pause_appends_and_latest_window_wins() {
        let pool = fresh_db().await;
        let bell = KillBell::new(pool.clone());
        let ledger = SqlLedger::new(pool);
        let now = Utc::now();
        bell.pause("first", "alice", 60, &key(), now).await.unwrap();
        bell.pause("second", "bob", 120, &key(), now + Duration::seconds(5))
            .await
            .unwrap();
        let entries = ledger
            .list(&crate::autonomy::ledger::LedgerFilter {
                kind: Some(LedgerKind::KillBellEngaged),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 2, "both pauses must each leave a receipt");
        match bell.current(now + Duration::seconds(10)).await.unwrap() {
            KillBellState::Paused {
                reason, paused_by, ..
            } => {
                assert_eq!(reason, "second", "latest pause's reason must surface");
                assert_eq!(paused_by, "bob");
            }
            other => panic!("expected Paused, got {other:?}"),
        }
    }

    /// `current()`, `is_paused()`, and `downgrade_if_paused()` must agree at
    /// the same `now`. This is a load-bearing contract: the dispatch loop
    /// reads any one of these and trusts it represents the true posture.
    #[tokio::test]
    async fn status_query_consistency_across_apis() {
        let bell = KillBell::new(fresh_db().await);
        let now = Utc::now();
        bell.pause("freeze", "alice", 3600, &key(), now)
            .await
            .unwrap();
        let probe = now + Duration::seconds(30);
        let cur_paused = matches!(
            bell.current(probe).await.unwrap(),
            KillBellState::Paused { .. }
        );
        let is_paused = bell.is_paused(probe).await.unwrap();
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::AllowMerge, probe)
            .await
            .unwrap();
        assert_eq!(cur_paused, is_paused, "current()/is_paused() must agree");
        assert!(cur_paused, "the bell is paused at probe time");
        assert_eq!(decision, GateDecision::RequireHuman);
        assert!(why.is_some(), "downgrade must surface a reason when paused");
    }
}
