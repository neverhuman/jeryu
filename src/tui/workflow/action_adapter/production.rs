//! Owner: Interactive TUI subsystem — Mission Control production adapter (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants: Only place the TUI imports `GitHubClient` / `SqlLedger`; sign + persist.

use std::sync::Arc;

use async_trait::async_trait;

use crate::autonomy::kill_bell::KillBell;
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::{GateDecision, LaunchLedgerEntry};
use crate::autonomy::{SqlLedger, sign_entry};
use crate::db::AnyPool;
use crate::git_host::{GitHost, GitHubClient, RepoRef};

use super::ActionAdapter;

/// Real-world adapter used by the live TUI. Cheap to clone (`github` is `Arc`,
/// `pool` is `AnyPool`-shaped, `signing_key` is `Arc`).
#[derive(Clone)]
pub struct ProductionActionAdapter {
    pub github: Arc<GitHubClient>,
    pub pool: AnyPool,
    pub signing_key: Arc<EdSigningKey>,
}

impl ProductionActionAdapter {
    pub fn new(github: Arc<GitHubClient>, pool: AnyPool, signing_key: Arc<EdSigningKey>) -> Self {
        Self {
            github,
            pool,
            signing_key,
        }
    }
}

#[async_trait]
impl ActionAdapter for ProductionActionAdapter {
    async fn post_passport_check(
        &self,
        repo: &str,
        head_sha: &str,
        decision: GateDecision,
        summary: &str,
    ) -> Result<String, String> {
        let repo_ref = RepoRef::parse(repo)
            .ok_or(format!("invalid repo slug '{repo}' (expected owner/name)"))?;
        let res = self
            .github
            .post_merge_passport_check(&repo_ref, head_sha, decision, summary, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(res.id)
    }

    async fn post_mr_comment(
        &self,
        repo: &str,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, String> {
        let repo_ref = RepoRef::parse(repo)
            .ok_or(format!("invalid repo slug '{repo}' (expected owner/name)"))?;
        self.github
            .post_mr_comment(&repo_ref, mr_iid, body)
            .await
            .map_err(|e| e.to_string())
    }

    async fn pause_kill_bell(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
    ) -> Result<(), String> {
        let bell = KillBell::new(self.pool.clone());
        bell.pause(
            reason,
            paused_by,
            ttl_seconds,
            self.signing_key.as_ref(),
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| e.to_string())
    }

    async fn append_ledger(&self, mut entry: LaunchLedgerEntry) -> Result<(), String> {
        // Sign and persist. The handler hands us an unsigned entry (stub
        // signature) so the adapter has a single, auditable place to apply
        // the operator's signing key.
        sign_entry(&mut entry, self.signing_key.as_ref());
        let ledger = SqlLedger::new(self.pool.clone());
        ledger.append(&entry).await.map_err(|e| e.to_string())
    }
}
