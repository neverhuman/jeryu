//! Kill Bell — global pause / break-glass for the autonomous control plane.
//!
//! Invariants:
//!   - Every pause carries a TTL; once `now >= expires_at` the bell auto-arms
//!     via [`KillBell::current`] even without an explicit `resume()`. This is
//!     load-bearing: a forgotten pause MUST NOT brick the control plane forever
//!     (R-5).
//!   - Every `pause()` / `resume()` appends a signed `KillBellEngaged` /
//!     `KillBellResumed` ledger entry through the [`VerdictLedger`] seam. Signing
//!     uses [`EdSigningKey`], so the ledger's stub/HMAC refusal automatically
//!     applies — no path lands an unsigned Kill Bell event.
//!   - While paused, [`KillBell::downgrade_if_paused`] rewrites any
//!     [`GateDecision`] to `RequireHuman`.

use crate::ledger::sign_entry;
use crate::seam::{SeamError, SeamResult, VerdictLedger};
use crate::signing::{EdSigningKey, Signature};
use crate::types::{GateDecision, LaunchLedgerEntry, LedgerKind, SchemaTag};
use chrono::{DateTime, Duration, Utc};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Current Kill Bell posture.
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

/// One physical state-transition row in the (append-only) history.
#[derive(Debug, Clone)]
enum Transition {
    Armed {
        at: DateTime<Utc>,
    },
    Paused {
        reason: String,
        paused_by: String,
        paused_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    },
}

impl Transition {
    fn at(&self) -> DateTime<Utc> {
        match self {
            Transition::Armed { at } => *at,
            Transition::Paused { paused_at, .. } => *paused_at,
        }
    }
}

/// Signed break-glass receipt. Minted by an operator who deliberately engages
/// or bypasses the Kill Bell for a bounded scope/window.
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

/// Kill Bell over the [`VerdictLedger`] seam. Cheap to clone (shared `Arc`s).
#[derive(Clone)]
pub struct KillBell {
    history: Arc<Mutex<Vec<Transition>>>,
    ledger: Arc<dyn VerdictLedger>,
}

impl KillBell {
    pub fn new(ledger: Arc<dyn VerdictLedger>) -> Self {
        Self {
            history: Arc::new(Mutex::new(Vec::new())),
            ledger,
        }
    }

    /// Read the most-recent transition. If the latest row is `Paused` but its
    /// TTL has elapsed (`expires_at <= now`), returns `Armed` (the
    /// auto-arm-on-TTL invariant). The physical row stays as an audit trail.
    pub async fn current(&self, now: DateTime<Utc>) -> SeamResult<KillBellState> {
        let history = self.history.lock().unwrap();
        let latest = history.iter().max_by(|a, b| a.at().cmp(&b.at())).cloned();
        Ok(match latest {
            None => KillBellState::Armed,
            Some(Transition::Armed { .. }) => KillBellState::Armed,
            Some(Transition::Paused {
                reason,
                paused_by,
                paused_at,
                expires_at,
            }) => {
                if now >= expires_at {
                    KillBellState::Armed
                } else {
                    KillBellState::Paused {
                        reason,
                        paused_by,
                        paused_at,
                        expires_at,
                    }
                }
            }
        })
    }

    /// Engage the bell. `ttl_seconds` bounds how long the pause holds before
    /// auto-arm. Appends a signed `KillBellEngaged` ledger entry BEFORE writing
    /// the state row, so the audit trail leads the state change.
    pub async fn pause(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
        signing_key: &EdSigningKey,
        now: DateTime<Utc>,
    ) -> SeamResult<()> {
        let ttl = ttl_seconds.min(i64::MAX as u64) as i64;
        let expires_at = now + Duration::seconds(ttl);

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
            signature: Signature::default_unsigned(),
        };
        sign_entry(&mut entry, signing_key);
        self.ledger
            .append(&entry)
            .await
            .map_err(|e| SeamError::new("kill_bell", format!("append KillBellEngaged: {e}")))?;

        self.history.lock().unwrap().push(Transition::Paused {
            reason: reason.to_string(),
            paused_by: paused_by.to_string(),
            paused_at: now,
            expires_at,
        });
        Ok(())
    }

    /// Resume normal operation. Appends a signed `KillBellResumed` ledger entry
    /// and writes an `Armed` row so `current()` reads back `Armed` even before
    /// the prior pause's TTL elapses.
    pub async fn resume(
        &self,
        resumed_by: &str,
        signing_key: &EdSigningKey,
        now: DateTime<Utc>,
    ) -> SeamResult<()> {
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
        self.ledger
            .append(&entry)
            .await
            .map_err(|e| SeamError::new("kill_bell", format!("append KillBellResumed: {e}")))?;

        self.history
            .lock()
            .unwrap()
            .push(Transition::Armed { at: now });
        Ok(())
    }

    /// Convenience: `true` iff `current(now)` is `Paused`.
    pub async fn is_paused(&self, now: DateTime<Utc>) -> SeamResult<bool> {
        Ok(self.current(now).await?.is_paused())
    }

    /// The hot-path check the dispatch loop runs before publishing a verdict.
    /// If paused, every decision downgrades to `RequireHuman` and the caller
    /// learns the reason; if armed, the decision passes through unchanged.
    pub async fn downgrade_if_paused(
        &self,
        decision: GateDecision,
        now: DateTime<Utc>,
    ) -> SeamResult<(GateDecision, Option<String>)> {
        match self.current(now).await? {
            KillBellState::Armed => Ok((decision, None)),
            KillBellState::Paused {
                reason, paused_by, ..
            } => {
                let detail = format!(
                    "kill bell engaged by '{paused_by}': {reason}; downgraded {decision:?} -> RequireHuman"
                );
                Ok((GateDecision::RequireHuman, Some(detail)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::MemoryLedger;
    use crate::seam::LedgerFilter;

    fn bell() -> (KillBell, Arc<MemoryLedger>) {
        let ledger = Arc::new(MemoryLedger::new());
        (KillBell::new(ledger.clone()), ledger)
    }

    fn key() -> EdSigningKey {
        EdSigningKey::generate("operator.kill-bell")
    }

    #[tokio::test]
    async fn pause_then_is_paused_true() {
        let (bell, _l) = bell();
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
        let (bell, _l) = bell();
        let t0 = Utc::now();
        bell.pause("short pause", "bob", 1, &key(), t0)
            .await
            .unwrap();
        assert!(bell.is_paused(t0).await.unwrap(), "paused at t0");
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
        let (bell, _l) = bell();
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
        let (bell, _l) = bell();
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
        let (bell, _l) = bell();
        let now = Utc::now();
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::AllowMerge, now)
            .await
            .unwrap();
        assert_eq!(decision, GateDecision::AllowMerge);
        assert!(why.is_none(), "armed must not surface a reason");
        let (decision, why) = bell
            .downgrade_if_paused(GateDecision::Reject, now)
            .await
            .unwrap();
        assert_eq!(decision, GateDecision::Reject);
        assert!(why.is_none());
    }

    #[tokio::test]
    async fn pause_appends_signed_ledger_entry_with_kill_bell_engaged_kind() {
        let (bell, ledger) = bell();
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
        assert_eq!(entries[0].subject_id, "kill_bell");
        assert_eq!(entries[0].actor, "alice");
        assert_eq!(entries[0].payload["reason"], "network split");
        assert_eq!(entries[0].payload["ttl_seconds"], 60);
        // The ledger entry is ed25519-signed (the ledger refuses anything else).
        assert_eq!(entries[0].signature.algo, "ed25519");
        assert_ne!(entries[0].signature.value, "0".repeat(64));
    }

    #[tokio::test]
    async fn resume_appends_ledger_entry_with_kill_bell_resumed_kind() {
        let (bell, ledger) = bell();
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
        assert_eq!(entries[0].actor, "bob");
        assert_eq!(entries[0].payload["resumed_by"], "bob");
    }

    #[tokio::test]
    async fn pause_during_pause_appends_and_latest_window_wins() {
        let (bell, ledger) = bell();
        let now = Utc::now();
        bell.pause("first", "alice", 60, &key(), now).await.unwrap();
        bell.pause("second", "bob", 120, &key(), now + Duration::seconds(5))
            .await
            .unwrap();
        let entries = ledger
            .list(&LedgerFilter {
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

    #[tokio::test]
    async fn status_query_consistency_across_apis() {
        let (bell, _l) = bell();
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
        assert!(cur_paused);
        assert_eq!(decision, GateDecision::RequireHuman);
        assert!(why.is_some());
    }
}
