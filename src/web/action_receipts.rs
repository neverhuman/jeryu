//! Web action receipt store (W-B-15 / §35.1.14 step 12).
//!
//! SQL-backed receipt log for the 14-step action execution contract. Each
//! call to [`WebActionReceiptStore::record`] writes a row to the
//! `web_action_receipts` SQLite table whose
//! `UNIQUE(action_kind, target_id, idempotency_key)` constraint enforces
//! single-execution at the durable layer. A best-effort JSONL line is
//! also appended to `target/jankurai/web/action_receipts.jsonl` so ops
//! tooling (jankurai, ux-qa) can tail receipts without DB access — the
//! DB write is authoritative.
//!
//! Schema mirror: `db/migrations/202606010001_web_forge_core.sql` and the
//! inline DDL in `db/state.rs::migrate`.

#![allow(clippy::too_many_arguments)]

use std::io::Write;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{AnyPool, Row};
use tracing::warn;

use crate::web::audit::RiskTier;

/// Default on-disk JSONL path. Relative so the file lands under the
/// invoking process's CWD (matches the `target/jankurai/` convention used
/// by `repo_ops.rs` / `repo_standard_render.rs`).
const DEFAULT_JSONL_PATH: &str = "target/jankurai/web/action_receipts.jsonl";

/// Status mirrored from the `web_action_receipts.status` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Accepted,
    Succeeded,
    Failed,
}

impl ReceiptStatus {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// Wire-shape mirroring the `web_action_receipts` columns. Serializable so
/// callers can return the receipt ID + summary alongside the action
/// response (§35.1.14 step 14).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebActionReceipt {
    pub id: String,
    pub actor_login: String,
    pub action_kind: String,
    pub target_kind: String,
    pub target_id: String,
    pub idempotency_key: Option<String>,
    pub expected_state_hash: Option<String>,
    pub resulting_state_hash: Option<String>,
    pub expected_sha: Option<String>,
    pub provider_calls: Vec<Value>,
    pub risk_tier: String,
    pub status: ReceiptStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl WebActionReceipt {
    pub fn new(
        actor_login: impl Into<String>,
        action_kind: impl Into<String>,
        target_kind: impl Into<String>,
        target_id: impl Into<String>,
        idempotency_key: Option<String>,
        expected_state_hash: Option<String>,
        resulting_state_hash: Option<String>,
        risk_tier: RiskTier,
        status: ReceiptStatus,
    ) -> Self {
        Self {
            id: format!("rcpt_{}", uuid::Uuid::new_v4().simple()),
            actor_login: actor_login.into(),
            action_kind: action_kind.into(),
            target_kind: target_kind.into(),
            target_id: target_id.into(),
            idempotency_key,
            expected_state_hash,
            resulting_state_hash,
            expected_sha: None,
            provider_calls: Vec::new(),
            risk_tier: risk_tier.label().to_string(),
            status,
            error: None,
            created_at: Utc::now(),
        }
    }
}

/// Subset of `web_action_receipts` columns returned by
/// [`WebActionReceiptStore::find_by_idempotency`]. The full
/// [`WebActionReceipt`] is reserved for write-side construction; the
/// idempotency-replay path only needs the identifying + status fields to
/// short-circuit a retry.
#[derive(Debug, Clone)]
pub struct ActionReceiptRow {
    pub id: String,
    pub status: ReceiptStatus,
    pub resulting_state_hash: Option<String>,
    pub error: Option<String>,
}

/// SQL-backed receipt log. Cheap to clone — the inner pool is `Arc<...>`
/// and the JSONL sink is a `PathBuf`.
#[derive(Clone)]
pub struct WebActionReceiptStore {
    pool: AnyPool,
    jsonl_path: Option<PathBuf>,
}

impl WebActionReceiptStore {
    /// Construct a store that writes to `pool` and stamps a JSONL fallback
    /// at the default `target/jankurai/web/action_receipts.jsonl` path.
    pub fn new(pool: AnyPool) -> Self {
        Self {
            pool,
            jsonl_path: Some(PathBuf::from(DEFAULT_JSONL_PATH)),
        }
    }

    /// Construct a store that skips the JSONL stamp (useful for unit
    /// tests that should not touch the filesystem).
    pub fn without_jsonl(pool: AnyPool) -> Self {
        Self {
            pool,
            jsonl_path: None,
        }
    }

    /// Override the JSONL sink path (used by tests).
    pub fn with_jsonl_path(mut self, path: PathBuf) -> Self {
        self.jsonl_path = Some(path);
        self
    }

    /// Persist a receipt row. Honors the
    /// `UNIQUE(action_kind, target_id, idempotency_key)` constraint via
    /// `ON CONFLICT … DO NOTHING`; callers that need true idempotency
    /// should first call [`Self::find_by_idempotency`] to short-circuit
    /// the executor.
    ///
    /// Returns the receipt with its mint-time `id`; the JSONL stamp is
    /// best-effort and never blocks the SQL write.
    pub async fn record(&self, receipt: WebActionReceipt) -> Result<WebActionReceipt, sqlx::Error> {
        let provider_calls_str =
            serde_json::Value::Array(receipt.provider_calls.clone()).to_string();
        let created_at = receipt.created_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO web_action_receipts
                (id, actor_login, action_kind, target_kind, target_id,
                 idempotency_key, expected_state_hash, resulting_state_hash,
                 expected_sha, provider_calls_json, risk_tier, status, error,
                 created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(action_kind, target_id, idempotency_key) DO NOTHING",
        )
        .bind(&receipt.id)
        .bind(&receipt.actor_login)
        .bind(&receipt.action_kind)
        .bind(&receipt.target_kind)
        .bind(&receipt.target_id)
        .bind(receipt.idempotency_key.as_deref())
        .bind(receipt.expected_state_hash.as_deref())
        .bind(receipt.resulting_state_hash.as_deref())
        .bind(receipt.expected_sha.as_deref())
        .bind(&provider_calls_str)
        .bind(&receipt.risk_tier)
        .bind(receipt.status.label())
        .bind(receipt.error.as_deref())
        .bind(&created_at)
        .execute(&self.pool)
        .await?;

        if let Some(path) = self.jsonl_path.as_deref() {
            stamp_jsonl(path, &receipt);
        }

        Ok(receipt)
    }

    /// Look up an existing receipt row keyed by
    /// `(action_kind, target_id, idempotency_key)`. Returns `Ok(None)` if
    /// the triple has never been recorded.
    pub async fn find_by_idempotency(
        &self,
        action_kind: &str,
        target_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<ActionReceiptRow>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, status, resulting_state_hash, error
             FROM web_action_receipts
             WHERE action_kind = ? AND target_id = ? AND idempotency_key = ?
             LIMIT 1",
        )
        .bind(action_kind)
        .bind(target_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else { return Ok(None) };

        let id: String = row.try_get("id")?;
        let status_s: String = row.try_get("status")?;
        let resulting_state_hash: Option<String> = row.try_get("resulting_state_hash")?;
        let error: Option<String> = row.try_get("error")?;
        let status = match status_s.as_str() {
            "accepted" => ReceiptStatus::Accepted,
            "succeeded" => ReceiptStatus::Succeeded,
            "failed" => ReceiptStatus::Failed,
            other => {
                warn!(
                    target: "jeryu.web.action_receipts",
                    status = %other,
                    "unrecognized receipt status; coercing to Accepted"
                );
                ReceiptStatus::Accepted
            }
        };
        Ok(Some(ActionReceiptRow {
            id,
            status,
            resulting_state_hash,
            error,
        }))
    }
}

/// Append a JSONL row for `receipt`. Failures (IO, permissions, etc.)
/// emit a tracing warning and are otherwise swallowed — JSONL is a
/// best-effort secondary trail; the DB write is authoritative.
fn stamp_jsonl(path: &std::path::Path, receipt: &WebActionReceipt) {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        warn!(
            target: "jeryu.web.action_receipts",
            path = %parent.display(),
            error = %err,
            "failed to create receipt directory"
        );
        return;
    }
    let line = match serde_json::to_string(receipt) {
        Ok(s) => s,
        Err(err) => {
            warn!(
                target: "jeryu.web.action_receipts",
                receipt_id = %receipt.id,
                error = %err,
                "failed to serialize receipt to JSONL"
            );
            return;
        }
    };
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(f) => f,
        Err(err) => {
            warn!(
                target: "jeryu.web.action_receipts",
                path = %path.display(),
                error = %err,
                "failed to open receipt JSONL file"
            );
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        warn!(
            target: "jeryu.web.action_receipts",
            path = %path.display(),
            error = %err,
            "failed to append receipt JSONL row"
        );
    }
}

/// Compute a stable state hash over an action `target_id` + caller-supplied
/// `params`. The hash is deterministic over a JSON-canonical key order so
/// repeated calls against unchanged state produce the same digest, which
/// matches the §35.1.14 step-12 `expected_state_hash` contract.
pub fn compute_state_hash(target_id: &str, params: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"jeryu:web.action:");
    h.update(target_id.as_bytes());
    h.update(b":");
    let canon = canonicalize(params);
    h.update(canon.as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

fn canonicalize(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut s = String::from("{");
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push('"');
                s.push_str(k);
                s.push_str("\":");
                s.push_str(&canonicalize(&map[*k]));
            }
            s.push('}');
            s
        }
        Value::Array(items) => {
            let mut s = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&canonicalize(item));
            }
            s.push(']');
            s
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu::state::Db;

    async fn fresh_store() -> WebActionReceiptStore {
        let db = Db::open_memory().await.expect("open mem db");
        WebActionReceiptStore::without_jsonl(db.pool())
    }

    fn sample_receipt(idem: &str) -> WebActionReceipt {
        WebActionReceipt::new(
            "@alice",
            "open_logs",
            "action",
            "open_logs",
            Some(idem.into()),
            Some("sha256:before".into()),
            Some("sha256:after".into()),
            RiskTier::Low,
            ReceiptStatus::Accepted,
        )
    }

    #[tokio::test]
    async fn record_persists_row() {
        let store = fresh_store().await;
        let receipt = sample_receipt("key-1");
        let id = receipt.id.clone();
        let stored = store.record(receipt).await.expect("record ok");
        assert_eq!(stored.id, id);
        // The row should now be reachable by idempotency lookup.
        let row = store
            .find_by_idempotency("open_logs", "open_logs", "key-1")
            .await
            .expect("find ok")
            .expect("row present");
        assert_eq!(row.id, id);
        assert!(matches!(row.status, ReceiptStatus::Accepted));
    }

    #[tokio::test]
    async fn record_is_idempotent_on_unique_collision() {
        let store = fresh_store().await;
        let r1 = sample_receipt("key-A");
        let r1_id = r1.id.clone();
        store.record(r1).await.expect("first record");

        // Second insert with the same (action_kind, target_id, key) is a
        // no-op thanks to the UNIQUE constraint + ON CONFLICT DO NOTHING.
        let r2 = sample_receipt("key-A");
        let r2_id = r2.id.clone();
        assert_ne!(r1_id, r2_id, "fresh receipts must mint distinct ids");
        store.record(r2).await.expect("second record (conflict)");

        // The stored row is still r1 (the original write).
        let row = store
            .find_by_idempotency("open_logs", "open_logs", "key-A")
            .await
            .expect("find ok")
            .expect("row present");
        assert_eq!(row.id, r1_id, "ON CONFLICT must preserve the original row");
    }

    #[tokio::test]
    async fn find_by_idempotency_returns_none_for_unknown_key() {
        let store = fresh_store().await;
        let hit = store
            .find_by_idempotency("open_logs", "open_logs", "missing")
            .await
            .expect("find ok");
        assert!(hit.is_none());
    }

    #[test]
    fn compute_state_hash_is_deterministic() {
        let p1 = serde_json::json!({"a": 1, "b": 2});
        let p2 = serde_json::json!({"b": 2, "a": 1});
        assert_eq!(
            compute_state_hash("target", &p1),
            compute_state_hash("target", &p2)
        );
        let p3 = serde_json::json!({"a": 1, "b": 3});
        assert_ne!(
            compute_state_hash("target", &p1),
            compute_state_hash("target", &p3)
        );
    }

    #[tokio::test]
    async fn jsonl_sink_appends_to_temp_file() {
        let db = Db::open_memory().await.expect("open mem db");
        let tmpdir = tempfile::tempdir().expect("tmpdir");
        let path = tmpdir.path().join("nested/receipts.jsonl");
        let store = WebActionReceiptStore::without_jsonl(db.pool()).with_jsonl_path(path.clone());
        let receipt = sample_receipt("key-jsonl");
        let id = receipt.id.clone();
        store.record(receipt).await.expect("record");
        let body = std::fs::read_to_string(&path).expect("file written");
        assert!(body.contains(&id));
        assert!(body.contains("open_logs"));
        assert!(body.ends_with('\n'));
    }
}
