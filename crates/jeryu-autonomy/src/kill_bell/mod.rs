//! Kill Bell — global pause / break-glass for the autonomous control plane.
//!
//! Invariants:
//!   - Every pause carries a TTL; once `now >= expires_at` the bell auto-arms
//!     via [`KillBell::current`] even without an explicit `resume()`. This is
//!     load-bearing: a forgotten pause MUST NOT brick the control plane forever
//!     (R-5).
//!   - Every `pause()` / `resume()` appends a signed `KillBellEngaged` /
//!     `KillBellResumed` ledger entry through the [`VerdictLedger`] seam. Signing
//!     uses [`EdSigningKey`], so the ledger's unsigned/HMAC refusal automatically
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
mod tests;
