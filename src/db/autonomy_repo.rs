//! Owner: db-boundary (Wave 11.A)
//! Proof: `cargo test -p jeryu --lib -- db::autonomy_repo`
//!
//! Typed repo that owns every `sqlx::query` previously scattered across
//! the autonomy modules. Closes `HLT-006-DIRECT-DB-WRONG-LAYER`: the
//! Sql* wrapper types in `src/autonomy/` no longer touch `sqlx::` —
//! they hold an `AutonomyRepo` and forward typed method calls here.
//!
//! Invariants preserved (mirror of the original Sql* docs):
//!   - `ledger_append` refuses stub / hmac signatures; idempotent on id.
//!   - `kill_bell_current` auto-arms when `now >= expires_at` even if
//!     the most recent row is still flagged "paused".
//!   - `verdict_save` marks prior non-superseded rows for the same
//!     (repo, merge_request) pair as superseded BEFORE the new INSERT.
//!   - `verdict_list_active` excludes expired, superseded, and rejected
//!     verdicts; orders by `created_at ASC`.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use sqlx::AnyPool;
use sqlx::Row;
use sqlx::any::AnyRow;

use crate::autonomy::signing::Signature;
use crate::autonomy::types::{
    GateDecision, LaunchLedgerEntry, LedgerKind, RiskTier, SchemaTag, VibeGateVerdict,
};

/// Filter passed to `ledger_list`. Mirrors the public `LedgerFilter` in
/// `src/autonomy/ledger.rs` to keep the repo independent of the wrapper.
#[derive(Debug, Clone, Default)]
pub struct LedgerFilter {
    pub kind: Option<LedgerKind>,
    pub subject_id: Option<String>,
    pub repo: Option<String>,
    pub limit: Option<i64>,
}

/// Kill Bell posture as read from the most-recent state-transition row
/// (auto-armed when TTL has elapsed). Mirrors `KillBellState` in
/// `src/autonomy/kill_bell.rs`.
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

/// Typed repo that owns every autonomy-side query.
#[derive(Debug, Clone)]
pub struct AutonomyRepo {
    pool: AnyPool,
}

impl AutonomyRepo {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    // ---------------------------------------------------------------
    // Launch ledger (formerly `SqlLedger`)
    // ---------------------------------------------------------------

    /// Append one entry. Refuses stub / hmac signatures (tip1 Law 10).
    /// Idempotent: same `entry.id` re-appended is a no-op (`INSERT OR IGNORE`).
    pub async fn ledger_append(&self, entry: &LaunchLedgerEntry) -> Result<()> {
        match entry.signature.algo.as_str() {
            "stub" | "sha256-hmac-stub" => bail!(
                "launch_ledger refuses entry signed with '{}'; enforcement mode requires ed25519",
                entry.signature.algo
            ),
            "" => bail!("launch_ledger refuses entry with empty signature algo"),
            _ => {}
        }
        let kind_str = kind_to_str(entry.kind);
        let payload =
            serde_json::to_string(&entry.payload).context("serialize ledger entry payload")?;
        sqlx::query(
            "INSERT OR IGNORE INTO launch_ledger
                 (id, kind, subject_id, repo, actor, payload,
                  signature_algo, signature_key_id, signature_value, recorded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.id)
        .bind(kind_str)
        .bind(&entry.subject_id)
        .bind(entry.repo.as_deref())
        .bind(&entry.actor)
        .bind(&payload)
        .bind(&entry.signature.algo)
        .bind(&entry.signature.key_id)
        .bind(&entry.signature.value)
        .bind(entry.recorded_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert launch_ledger row")?;
        Ok(())
    }

    /// Return entries matching the filter, oldest first.
    pub async fn ledger_list(&self, filter: &LedgerFilter) -> Result<Vec<LaunchLedgerEntry>> {
        // Build SQL with bind placeholders; do NOT inline strings (avoid injection).
        let mut where_clauses = Vec::<&'static str>::new();
        if filter.kind.is_some() {
            where_clauses.push("kind = ?");
        }
        if filter.subject_id.is_some() {
            where_clauses.push("subject_id = ?");
        }
        if filter.repo.is_some() {
            where_clauses.push("repo = ?");
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };
        let limit_sql = filter
            .limit
            .map_or_else(String::new, |n| format!(" LIMIT {n}"));
        let sql = format!(
            "SELECT id, kind, subject_id, repo, actor, payload,
                    signature_algo, signature_key_id, signature_value, recorded_at
             FROM launch_ledger{where_sql}
             ORDER BY recorded_at ASC{limit_sql}"
        );
        let mut q = sqlx::query(&sql);
        if let Some(k) = filter.kind {
            q = q.bind(kind_to_str(k));
        }
        if let Some(s) = &filter.subject_id {
            q = q.bind(s.as_str());
        }
        if let Some(r) = &filter.repo {
            q = q.bind(r.as_str());
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .context("query launch_ledger")?;
        rows.into_iter().map(row_to_entry).collect()
    }

    // ---------------------------------------------------------------
    // Kill bell (formerly `KillBell::current/pause/resume`)
    // ---------------------------------------------------------------

    /// Read the most-recent state transition. If the latest row is
    /// `paused` but its TTL has elapsed (`expires_at <= now`), returns
    /// `Armed` — auto-arm-on-TTL invariant.
    pub async fn kill_bell_current(&self, now: DateTime<Utc>) -> Result<KillBellState> {
        let row = sqlx::query(
            "SELECT state, reason, paused_by, paused_at, expires_at
             FROM kill_bell_state
             ORDER BY set_at DESC
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("query kill_bell_state")?;
        let Some(row) = row else {
            return Ok(KillBellState::Armed);
        };
        let state: String = row.try_get("state").context("read kill_bell_state.state")?;
        if state != "paused" {
            return Ok(KillBellState::Armed);
        }
        let reason: String = match row.try_get("reason") {
            Ok(reason) => reason,
            Err(_) => String::new(),
        };
        let paused_by: String = match row.try_get("paused_by") {
            Ok(paused_by) => paused_by,
            Err(_) => String::new(),
        };
        let paused_at_str: String = row
            .try_get("paused_at")
            .context("read kill_bell_state.paused_at")?;
        let expires_at_str: String = row
            .try_get("expires_at")
            .context("read kill_bell_state.expires_at")?;
        let paused_at = parse_rfc3339(&paused_at_str, "paused_at")?;
        let expires_at = parse_rfc3339(&expires_at_str, "expires_at")?;
        if now >= expires_at {
            return Ok(KillBellState::Armed);
        }
        Ok(KillBellState::Paused {
            reason,
            paused_by,
            paused_at,
            expires_at,
        })
    }

    /// Write a `paused` row. The caller is responsible for appending the
    /// signed `KillBellEngaged` ledger entry separately.
    pub async fn kill_bell_set_paused(
        &self,
        reason: &str,
        paused_by: &str,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO kill_bell_state
                 (state, reason, paused_by, paused_at, expires_at, set_at)
             VALUES ('paused', ?, ?, ?, ?, ?)",
        )
        .bind(reason)
        .bind(paused_by)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert kill_bell_state paused row")?;
        Ok(())
    }

    /// Write an `armed` row. The caller is responsible for appending the
    /// signed `KillBellResumed` ledger entry separately.
    pub async fn kill_bell_set_armed(&self, resumed_by: &str, now: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "INSERT INTO kill_bell_state
                 (state, reason, paused_by, paused_at, expires_at, set_at)
             VALUES ('armed', NULL, ?, NULL, NULL, ?)",
        )
        .bind(resumed_by)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert kill_bell_state armed row")?;
        Ok(())
    }

    // ---------------------------------------------------------------
    // Verdicts (formerly `SqlVerdictStore`)
    // ---------------------------------------------------------------

    /// Persist a verdict. Marks prior non-superseded rows for the same
    /// (repo, merge_request) pair as superseded BEFORE the new INSERT.
    /// Idempotent on `verdict.id`.
    pub async fn verdict_save(&self, verdict: &VibeGateVerdict) -> Result<()> {
        let save_at = Utc::now().to_rfc3339();
        let body_json =
            serde_json::to_string(verdict).context("serialize VibeGateVerdict body_json")?;

        match verdict.merge_request.as_deref() {
            Some(mr) => {
                sqlx::query(
                    "UPDATE verdicts SET superseded_at = ?
                     WHERE repo = ? AND merge_request = ? AND superseded_at IS NULL
                       AND id != ?",
                )
                .bind(&save_at)
                .bind(&verdict.repo)
                .bind(mr)
                .bind(&verdict.id)
                .execute(&self.pool)
                .await
                .context("mark prior verdicts superseded (with mr)")?;
            }
            None => {
                sqlx::query(
                    "UPDATE verdicts SET superseded_at = ?
                     WHERE repo = ? AND merge_request IS NULL AND superseded_at IS NULL
                       AND id != ?",
                )
                .bind(&save_at)
                .bind(&verdict.repo)
                .bind(&verdict.id)
                .execute(&self.pool)
                .await
                .context("mark prior verdicts superseded (null mr)")?;
            }
        }

        sqlx::query(
            "INSERT OR IGNORE INTO verdicts
                 (id, repo, merge_request, head_sha, policy_sha, target_branch,
                  risk, decision, expires_at, created_at, body_json,
                  signature_algo, signature_key_id, signature_value, superseded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(&verdict.id)
        .bind(&verdict.repo)
        .bind(verdict.merge_request.as_deref())
        .bind(&verdict.head_sha)
        .bind(&verdict.policy_sha)
        .bind(&verdict.target_branch)
        .bind(risk_to_str(verdict.risk))
        .bind(decision_to_str(verdict.decision))
        .bind(verdict.expires_at.to_rfc3339())
        .bind(verdict.created_at.to_rfc3339())
        .bind(&body_json)
        .bind(&verdict.signature.algo)
        .bind(&verdict.signature.key_id)
        .bind(&verdict.signature.value)
        .execute(&self.pool)
        .await
        .context("insert verdicts row")?;
        Ok(())
    }

    /// Return the most-recent non-superseded verdict for a (repo, mr) pair.
    pub async fn verdict_load_latest(
        &self,
        repo: &str,
        merge_request: Option<&str>,
    ) -> Result<Option<VibeGateVerdict>> {
        let row_opt = match merge_request {
            Some(mr) => sqlx::query(
                "SELECT body_json FROM verdicts
                     WHERE repo = ? AND merge_request = ? AND superseded_at IS NULL
                     ORDER BY created_at DESC
                     LIMIT 1",
            )
            .bind(repo)
            .bind(mr)
            .fetch_optional(&self.pool)
            .await
            .context("load_latest with mr")?,
            None => sqlx::query(
                "SELECT body_json FROM verdicts
                     WHERE repo = ? AND merge_request IS NULL AND superseded_at IS NULL
                     ORDER BY created_at DESC
                     LIMIT 1",
            )
            .bind(repo)
            .fetch_optional(&self.pool)
            .await
            .context("load_latest with null mr")?,
        };
        match row_opt {
            None => Ok(None),
            Some(row) => Ok(Some(decode_body(&row)?)),
        }
    }

    /// Return all currently-active verdicts: not superseded, not expired,
    /// and not `reject`. Ordered by `created_at` ascending.
    pub async fn verdict_list_active(&self, now: DateTime<Utc>) -> Result<Vec<VibeGateVerdict>> {
        let rows = sqlx::query(
            "SELECT body_json FROM verdicts
             WHERE superseded_at IS NULL
               AND expires_at > ?
               AND decision != 'reject'
             ORDER BY created_at ASC",
        )
        .bind(now.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .context("list_active query")?;
        rows.iter().map(decode_body).collect()
    }

    /// Mark one specific verdict row as superseded. No-op if already
    /// superseded or if the id is unknown.
    pub async fn verdict_supersede(&self, verdict_id: &str, now: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "UPDATE verdicts SET superseded_at = ?
             WHERE id = ? AND superseded_at IS NULL",
        )
        .bind(now.to_rfc3339())
        .bind(verdict_id)
        .execute(&self.pool)
        .await
        .context("supersede update")?;
        Ok(())
    }

    /// Return every ledger row appended since `since`, ascending. Used
    /// by the metrics aggregator.
    pub async fn metrics_ledger_rows_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<LaunchLedgerEntry>> {
        let rows = sqlx::query(
            "SELECT id, kind, subject_id, repo, actor, payload,
                    signature_algo, signature_key_id, signature_value, recorded_at
             FROM launch_ledger
             WHERE recorded_at >= ?
             ORDER BY recorded_at ASC",
        )
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .context("query launch_ledger since")?;
        rows.into_iter().map(row_to_entry).collect()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn decision_to_str(d: GateDecision) -> &'static str {
    match d {
        GateDecision::AllowMerge => "allow_merge",
        GateDecision::RequireHuman => "require_human",
        GateDecision::Reject => "reject",
    }
}

fn risk_to_str(r: RiskTier) -> &'static str {
    match r {
        RiskTier::R0 => "R0",
        RiskTier::R1 => "R1",
        RiskTier::R2 => "R2",
        RiskTier::R3 => "R3",
        RiskTier::R4 => "R4",
        RiskTier::R5 => "R5",
    }
}

fn parse_rfc3339(s: &str, field: &'static str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("parse kill_bell_state.{field} as rfc3339"))?
        .with_timezone(&Utc))
}

fn kind_to_str(k: LedgerKind) -> &'static str {
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

fn kind_from_str(s: &str) -> Result<LedgerKind> {
    Ok(match s {
        "intent_declared" => LedgerKind::IntentDeclared,
        "lease_issued" => LedgerKind::LeaseIssued,
        "lease_expired" => LedgerKind::LeaseExpired,
        "evidence_pack_created" => LedgerKind::EvidencePackCreated,
        "review_started" => LedgerKind::ReviewStarted,
        "review_completed" => LedgerKind::ReviewCompleted,
        "verdict_issued" => LedgerKind::VerdictIssued,
        "merge_passport_issued" => LedgerKind::MergePassportIssued,
        "merge_passport_consumed" => LedgerKind::MergePassportConsumed,
        "merge_passport_invalidated" => LedgerKind::MergePassportInvalidated,
        "release_passport_issued" => LedgerKind::ReleasePassportIssued,
        "deployment_started" => LedgerKind::DeploymentStarted,
        "deployment_promoted" => LedgerKind::DeploymentPromoted,
        "rollback_initiated" => LedgerKind::RollbackInitiated,
        "rollback_completed" => LedgerKind::RollbackCompleted,
        "human_escalation_requested" => LedgerKind::HumanEscalationRequested,
        "human_decision_recorded" => LedgerKind::HumanDecisionRecorded,
        "webhook_received" => LedgerKind::WebhookReceived,
        "autonomy_pack_edit_proposed" => LedgerKind::AutonomyPackEditProposed,
        "autonomy_pack_edit_merged" => LedgerKind::AutonomyPackEditMerged,
        "kill_bell_engaged" => LedgerKind::KillBellEngaged,
        "kill_bell_resumed" => LedgerKind::KillBellResumed,
        other => bail!("unknown launch_ledger kind: {other}"),
    })
}

fn row_to_entry(row: AnyRow) -> Result<LaunchLedgerEntry> {
    let id: String = row.try_get("id")?;
    let kind_str: String = row.try_get("kind")?;
    let subject_id: String = row.try_get("subject_id")?;
    let repo: Option<String> = row.try_get("repo").ok();
    let actor: String = row.try_get("actor")?;
    let payload_str: String = row.try_get("payload")?;
    let signature_algo: String = row.try_get("signature_algo")?;
    let signature_key_id: String = row.try_get("signature_key_id")?;
    let signature_value: String = row.try_get("signature_value")?;
    let recorded_at_str: String = row.try_get("recorded_at")?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str)
        .with_context(|| format!("decode ledger payload for id={id}"))?;
    let recorded_at = chrono::DateTime::parse_from_rfc3339(&recorded_at_str)
        .with_context(|| format!("parse recorded_at for id={id}"))?
        .with_timezone(&chrono::Utc);
    Ok(LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id,
        kind: kind_from_str(&kind_str)?,
        subject_id,
        repo,
        payload,
        recorded_at,
        actor,
        signature: Signature {
            algo: signature_algo,
            key_id: signature_key_id,
            value: signature_value,
        },
    })
}

/// Decode `body_json` back into a `VibeGateVerdict`.
fn decode_body(row: &AnyRow) -> Result<VibeGateVerdict> {
    let body_str: String = row.try_get("body_json").context("read body_json column")?;
    serde_json::from_str(&body_str).context("decode VibeGateVerdict from body_json")
}

// ---------------------------------------------------------------------------
// In-memory test schema installer. Public(crate) so the Sql* wrapper tests
// (and tests living inside the autonomy crate) can spin up an in-memory pool
// without importing `sqlx::` themselves. This is the test-support seam: it
// runs only with `cfg(test)` to keep production builds slim.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) async fn fresh_autonomy_pool() -> AnyPool {
    use crate::db::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("redline::memory:")
        .await
        .expect("connect in-memory redline");
    for stmt in autonomy_schema_ddl() {
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }
    pool
}

/// DDL needed for the autonomy tables (launch_ledger + kill_bell_state +
/// verdicts). Mirrors the production schema in `db/state.rs::migrate`.
#[cfg(test)]
pub(crate) fn autonomy_schema_ddl() -> &'static [&'static str] {
    &[
        "CREATE TABLE launch_ledger (
            id TEXT PRIMARY KEY, kind TEXT NOT NULL, subject_id TEXT NOT NULL,
            repo TEXT, actor TEXT NOT NULL, payload TEXT NOT NULL,
            signature_algo TEXT NOT NULL, signature_key_id TEXT NOT NULL,
            signature_value TEXT NOT NULL, recorded_at TEXT NOT NULL)",
        "CREATE TRIGGER launch_ledger_no_update
             BEFORE UPDATE ON launch_ledger
         BEGIN SELECT RAISE(ABORT, 'launch_ledger is append-only'); END",
        "CREATE TRIGGER launch_ledger_no_delete
             BEFORE DELETE ON launch_ledger
         BEGIN SELECT RAISE(ABORT, 'launch_ledger is append-only'); END",
        "CREATE TABLE kill_bell_state (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            state TEXT NOT NULL CHECK(state IN ('armed','paused')),
            reason TEXT, paused_by TEXT, paused_at TEXT, expires_at TEXT,
            set_at TEXT NOT NULL)",
        "CREATE INDEX idx_kill_bell_state_set_at
             ON kill_bell_state(set_at DESC)",
        "CREATE TABLE verdicts (
            id TEXT PRIMARY KEY,
            repo TEXT NOT NULL,
            merge_request TEXT,
            head_sha TEXT NOT NULL,
            policy_sha TEXT NOT NULL,
            target_branch TEXT NOT NULL,
            risk TEXT NOT NULL,
            decision TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            body_json TEXT NOT NULL,
            signature_algo TEXT NOT NULL,
            signature_key_id TEXT NOT NULL,
            signature_value TEXT NOT NULL,
            superseded_at TEXT)",
        "CREATE INDEX idx_verdicts_repo_mr
             ON verdicts(repo, merge_request, created_at DESC)",
        "CREATE INDEX idx_verdicts_active
             ON verdicts(expires_at, superseded_at)",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::{EdSigningKey, Signature};
    use crate::autonomy::types::{
        GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
    };
    use chrono::Duration;

    fn signed_entry(id: &str, kind: LedgerKind) -> LaunchLedgerEntry {
        let key = EdSigningKey::generate("repo-test");
        let body = format!("{id}|{:?}", kind);
        let sig = key.sign_raw(body.as_bytes());
        LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: id.into(),
            kind,
            subject_id: "subj-1".into(),
            repo: Some("owner/repo".into()),
            payload: serde_json::json!({"hello": "world"}),
            recorded_at: Utc::now(),
            actor: "judge.v1".into(),
            signature: sig,
        }
    }

    fn mint_verdict(
        repo: &str,
        mr: Option<&str>,
        head_sha_tail: &str,
        decision: GateDecision,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> VibeGateVerdict {
        let head_sha = format!("{head_sha_tail:0>40}");
        let id = format!(
            "vgv_{}_{}",
            created_at.timestamp_millis(),
            &head_sha[head_sha.len().saturating_sub(8)..]
        );
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id,
            evidence_pack_id: "ep_test".into(),
            merge_request: mr.map(|s| s.to_string()),
            repo: repo.into(),
            target_branch: "main".into(),
            head_sha,
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at,
            created_at,
            signature: Signature::stub(),
        }
    }

    // -- 1. ledger_append_then_list -------------------------------------

    #[tokio::test]
    async fn ledger_append_then_list_returns_one_row() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let e = signed_entry("evt-1", LedgerKind::VerdictIssued);
        repo.ledger_append(&e).await.unwrap();
        let got = repo.ledger_list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "evt-1");
        assert_eq!(got[0].kind, LedgerKind::VerdictIssued);
    }

    // -- 2. ledger_append_refuses_stub_signature ------------------------

    #[tokio::test]
    async fn ledger_append_refuses_stub_signature() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let mut e = signed_entry("evt-stub", LedgerKind::VerdictIssued);
        e.signature = Signature::stub();
        let err = repo.ledger_append(&e).await.unwrap_err();
        assert!(err.to_string().contains("stub"), "actual: {err}");
    }

    // -- 3. kill_bell_current_returns_armed_when_no_rows ---------------

    #[tokio::test]
    async fn kill_bell_current_returns_armed_when_no_rows() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let state = repo.kill_bell_current(Utc::now()).await.unwrap();
        assert_eq!(state, KillBellState::Armed);
    }

    // -- 4. kill_bell_set_paused_then_current_returns_paused -----------

    #[tokio::test]
    async fn kill_bell_set_paused_then_current_returns_paused() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let now = Utc::now();
        let expires = now + Duration::minutes(30);
        repo.kill_bell_set_paused("brown alert", "alice", expires, now)
            .await
            .unwrap();
        let state = repo.kill_bell_current(now).await.unwrap();
        match state {
            KillBellState::Paused {
                reason, paused_by, ..
            } => {
                assert_eq!(reason, "brown alert");
                assert_eq!(paused_by, "alice");
            }
            other => panic!("expected Paused, got {other:?}"),
        }
    }

    // -- 5. kill_bell auto-arm after TTL expires -----------------------

    #[tokio::test]
    async fn kill_bell_auto_arms_after_ttl_elapses() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let t0 = Utc::now();
        repo.kill_bell_set_paused("blip", "ops", t0 + Duration::seconds(1), t0)
            .await
            .unwrap();
        let later = t0 + Duration::minutes(5);
        assert_eq!(
            repo.kill_bell_current(later).await.unwrap(),
            KillBellState::Armed
        );
    }

    // -- 6. verdict_save_then_load_latest -----------------------------

    #[tokio::test]
    async fn verdict_save_then_load_latest_roundtrips() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!42"),
            "abc12345",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        repo.verdict_save(&v).await.unwrap();
        let got = repo
            .verdict_load_latest("owner/repo", Some("!42"))
            .await
            .unwrap()
            .expect("must round-trip");
        assert_eq!(got.id, v.id);
    }

    // -- 7. verdict_list_active_excludes_expired ----------------------

    #[tokio::test]
    async fn verdict_list_active_excludes_expired() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let now = Utc::now();
        let expired = mint_verdict(
            "owner/repo",
            Some("!1"),
            "11110000",
            GateDecision::AllowMerge,
            now - Duration::hours(2),
            now - Duration::hours(1),
        );
        let live = mint_verdict(
            "owner/repo",
            Some("!2"),
            "22220000",
            GateDecision::AllowMerge,
            now,
            now + Duration::minutes(60),
        );
        repo.verdict_save(&expired).await.unwrap();
        repo.verdict_save(&live).await.unwrap();
        let active = repo.verdict_list_active(now).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, live.id);
    }

    // -- 8. concurrent_ledger_append_no_corruption -------------------

    #[tokio::test]
    async fn concurrent_ledger_append_no_corruption() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let mut handles = Vec::new();
        for task in 0..4 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("evt-t{task}-{i}");
                    let e = signed_entry(&id, LedgerKind::VerdictIssued);
                    repo.ledger_append(&e).await.expect("concurrent append");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        let got = repo.ledger_list(&LedgerFilter::default()).await.unwrap();
        assert_eq!(got.len(), 20);
    }

    // -- 9. metrics_ledger_rows_since filters ----------------------

    #[tokio::test]
    async fn metrics_ledger_rows_since_filters_by_time() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let now = Utc::now();
        // Append two entries; the metric filter at `now` should match both.
        let mut e1 = signed_entry("m1", LedgerKind::VerdictIssued);
        e1.recorded_at = now;
        let mut e2 = signed_entry("m2", LedgerKind::VerdictIssued);
        e2.recorded_at = now + Duration::seconds(1);
        repo.ledger_append(&e1).await.unwrap();
        repo.ledger_append(&e2).await.unwrap();
        let rows = repo
            .metrics_ledger_rows_since(now - Duration::seconds(1))
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        let rows = repo
            .metrics_ledger_rows_since(now + Duration::seconds(2))
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    // -- 10. verdict_supersede marks row -------------------------

    #[tokio::test]
    async fn verdict_supersede_drops_row_from_active_list() {
        let repo = AutonomyRepo::new(fresh_autonomy_pool().await);
        let now = Utc::now();
        let v = mint_verdict(
            "owner/repo",
            Some("!s"),
            "11111111",
            GateDecision::AllowMerge,
            now,
            now + Duration::hours(1),
        );
        repo.verdict_save(&v).await.unwrap();
        assert_eq!(repo.verdict_list_active(now).await.unwrap().len(), 1);
        repo.verdict_supersede(&v.id, now + Duration::seconds(5))
            .await
            .unwrap();
        assert_eq!(repo.verdict_list_active(now).await.unwrap().len(), 0);
        // Idempotent
        repo.verdict_supersede(&v.id, now + Duration::seconds(10))
            .await
            .unwrap();
    }
}
