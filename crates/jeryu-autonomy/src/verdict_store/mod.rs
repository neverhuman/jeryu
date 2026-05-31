//! Verdict persistence (the live-verdict projection the daemon polls).
//!
//! Invariants:
//!   - `save()` is idempotent on `verdict.id`.
//!   - Before inserting a new verdict for an existing (repo, pull_request) pair,
//!     every prior non-superseded row for that pair is marked superseded. This
//!     keeps `load_latest` cheap and gives `list_active` a single boolean.
//!   - `body_json` is the source of truth: the full [`VibeGateVerdict`]
//!     round-trips losslessly.
//!   - This store does NOT enforce signing — unlike the ledger. The daemon may
//!     persist unsigned verdicts here for replay/debug.

use crate::seam::{SeamResult, VerdictStore};
use crate::types::GateDecision;
use crate::types::VibeGateVerdict;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

/// One persisted row. `body` is the source of truth; `superseded_at` is the
/// single boolean `list_active` filters on.
#[derive(Clone)]
struct Row {
    body: VibeGateVerdict,
    superseded_at: Option<DateTime<Utc>>,
}

/// In-memory verdict store. Cheap to clone (shared `Arc`).
#[derive(Clone, Default)]
pub struct MemoryVerdictStore {
    rows: Arc<Mutex<Vec<Row>>>,
}

impl MemoryVerdictStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl VerdictStore for MemoryVerdictStore {
    async fn save(&self, verdict: &VibeGateVerdict) -> SeamResult<()> {
        let mut rows = self.rows.lock().unwrap();
        // Idempotent on id.
        if rows.iter().any(|r| r.body.id == verdict.id) {
            return Ok(());
        }
        // Supersede prior non-superseded rows for the same (repo, pull_request).
        for r in rows.iter_mut() {
            if r.superseded_at.is_none()
                && r.body.repo == verdict.repo
                && r.body.pull_request == verdict.pull_request
            {
                r.superseded_at = Some(verdict.created_at);
            }
        }
        rows.push(Row {
            body: verdict.clone(),
            superseded_at: None,
        });
        Ok(())
    }

    async fn load_latest(
        &self,
        repo: &str,
        pull_request: Option<&str>,
    ) -> SeamResult<Option<VibeGateVerdict>> {
        let rows = self.rows.lock().unwrap();
        let pr = pull_request.map(|s| s.to_string());
        let latest = rows
            .iter()
            .filter(|r| {
                r.superseded_at.is_none() && r.body.repo == repo && r.body.pull_request == pr
            })
            .max_by(|a, b| a.body.created_at.cmp(&b.body.created_at))
            .map(|r| r.body.clone());
        Ok(latest)
    }

    async fn list_active(&self, now: DateTime<Utc>) -> SeamResult<Vec<VibeGateVerdict>> {
        let rows = self.rows.lock().unwrap();
        let mut active: Vec<VibeGateVerdict> = rows
            .iter()
            .filter(|r| {
                r.superseded_at.is_none()
                    && r.body.expires_at > now
                    && r.body.decision != GateDecision::Reject
            })
            .map(|r| r.body.clone())
            .collect();
        active.sort_by_key(|v| v.created_at);
        Ok(active)
    }

    async fn supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> SeamResult<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.body.id == verdict_id)
            && r.superseded_at.is_none()
        {
            r.superseded_at = Some(now);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
