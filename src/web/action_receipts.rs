//! Web action receipt store (W-B-15 / §35.1.14 step 12).
//!
//! Phase 4 in-memory + JSONL-fallback receipt log for the 14-step action
//! execution contract. The canonical durable target is the
//! `web_action_receipts` SQLite table (added in migration W-F-05); the
//! runtime is not yet wired through `state.db_pool` from the BFF binary,
//! so this module:
//!
//!   * keeps the last N receipts in a `parking_lot::Mutex<VecDeque>` so
//!     in-process consumers (debug endpoints, tests) can read them back;
//!   * append-stamps every receipt as a JSONL row in
//!     `target/jankurai/web/action_receipts.jsonl` so out-of-process tools
//!     (jankurai, ux-qa) have a durable trail.
//!
//! Trade-off: the JSONL file is best-effort and does NOT participate in
//! the action transaction. Once the BFF gains access to the engine's
//! `sqlx` pool (W-B-* wiring), swap `JsonlReceiptSink` for a
//! `SqlxReceiptSink` writing to `web_action_receipts` with the
//! `UNIQUE(action_kind, target_id, idempotency_key)` constraint enforcing
//! single-execution at the durable layer.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::web::audit::RiskTier;

/// Cap on the rolling in-memory window — receipts older than this fall off
/// the VecDeque (the JSONL file is the durable record).
const MEMORY_CAPACITY: usize = 500;

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

/// Sink that durably stamps receipts somewhere. Phase 4 ships
/// [`JsonlReceiptSink`]; the SQLite-backed sink lands when the engine pool
/// is plumbed through `WebState`.
pub trait ReceiptSink: Send + Sync {
    fn stamp(&self, receipt: &WebActionReceipt);
}

/// JSONL append-stamp sink. Failures (IO, permissions, etc.) emit a
/// tracing warning and are otherwise swallowed — receipts must never
/// block the request path.
pub struct JsonlReceiptSink {
    path: PathBuf,
}

impl JsonlReceiptSink {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(DEFAULT_JSONL_PATH),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Default for JsonlReceiptSink {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiptSink for JsonlReceiptSink {
    fn stamp(&self, receipt: &WebActionReceipt) {
        if let Some(parent) = self.path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            warn!(
                target = "jeryu.web.action_receipts",
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
                    target = "jeryu.web.action_receipts",
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
            .open(&self.path)
        {
            Ok(f) => f,
            Err(err) => {
                warn!(
                    target = "jeryu.web.action_receipts",
                    path = %self.path.display(),
                    error = %err,
                    "failed to open receipt JSONL file"
                );
                return;
            }
        };
        if let Err(err) = writeln!(file, "{line}") {
            warn!(
                target = "jeryu.web.action_receipts",
                path = %self.path.display(),
                error = %err,
                "failed to append receipt JSONL row"
            );
        }
    }
}

/// In-memory rolling window of receipts. Backed by an optional [`ReceiptSink`]
/// so the in-memory log and the durable JSONL stamp stay in sync.
pub struct WebActionReceiptStore {
    inner: Mutex<VecDeque<WebActionReceipt>>,
    sink: Arc<dyn ReceiptSink>,
}

impl WebActionReceiptStore {
    pub fn new() -> Self {
        Self::with_sink(Arc::new(JsonlReceiptSink::new()))
    }

    pub fn with_sink(sink: Arc<dyn ReceiptSink>) -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(MEMORY_CAPACITY)),
            sink,
        }
    }

    /// Record a receipt — appends to the in-memory ring and stamps the
    /// durable sink. Returns the receipt for convenient chaining.
    pub fn record(&self, receipt: WebActionReceipt) -> WebActionReceipt {
        self.sink.stamp(&receipt);
        let mut guard = self.inner.lock();
        if guard.len() == MEMORY_CAPACITY {
            guard.pop_front();
        }
        guard.push_back(receipt.clone());
        receipt
    }

    /// Find the most-recent receipt for an
    /// (`action_kind`, `target_id`, `idempotency_key`) triple.
    /// Used to detect §35.1.16 idempotency conflicts when the same key
    /// recurs against a different payload.
    pub fn find_by_idempotency(
        &self,
        action_kind: &str,
        target_id: &str,
        idempotency_key: &str,
    ) -> Option<WebActionReceipt> {
        let guard = self.inner.lock();
        guard
            .iter()
            .rev()
            .find(|r| {
                r.action_kind == action_kind
                    && r.target_id == target_id
                    && r.idempotency_key.as_deref() == Some(idempotency_key)
            })
            .cloned()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

impl Default for WebActionReceiptStore {
    fn default() -> Self {
        Self::new()
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
    // BTreeMap-style canonicalization: re-serialize via `serde_json::to_value`
    // is already canonical at the top level, but `Value` keeps insertion
    // order for objects. Convert to a sorted BTreeMap representation for
    // stability.
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

    struct NoopSink;
    impl ReceiptSink for NoopSink {
        fn stamp(&self, _: &WebActionReceipt) {}
    }

    fn store_with_noop_sink() -> WebActionReceiptStore {
        WebActionReceiptStore::with_sink(Arc::new(NoopSink))
    }

    #[test]
    fn record_appends_to_memory() {
        let store = store_with_noop_sink();
        assert!(store.is_empty());
        let receipt = WebActionReceipt::new(
            "@alice",
            "open_logs",
            "action",
            "open_logs",
            Some("key-1".into()),
            None,
            None,
            RiskTier::Low,
            ReceiptStatus::Accepted,
        );
        let stored = store.record(receipt);
        assert_eq!(store.len(), 1);
        assert!(stored.id.starts_with("rcpt_"));
    }

    #[test]
    fn find_by_idempotency_returns_latest_match() {
        let store = store_with_noop_sink();
        let r1 = WebActionReceipt::new(
            "@alice",
            "open_logs",
            "action",
            "open_logs",
            Some("key-A".into()),
            None,
            None,
            RiskTier::Low,
            ReceiptStatus::Accepted,
        );
        store.record(r1.clone());
        let r2 = WebActionReceipt::new(
            "@alice",
            "open_logs",
            "action",
            "open_logs",
            Some("key-B".into()),
            None,
            None,
            RiskTier::Low,
            ReceiptStatus::Accepted,
        );
        store.record(r2.clone());
        let hit = store
            .find_by_idempotency("open_logs", "open_logs", "key-A")
            .expect("key-A receipt");
        assert_eq!(hit.id, r1.id);
        let miss = store.find_by_idempotency("open_logs", "open_logs", "missing");
        assert!(miss.is_none());
    }

    #[test]
    fn rolling_window_evicts_oldest() {
        let store = store_with_noop_sink();
        for i in 0..(MEMORY_CAPACITY + 5) {
            store.record(WebActionReceipt::new(
                "@alice",
                "open_logs",
                "action",
                format!("t-{i}"),
                None,
                None,
                None,
                RiskTier::Low,
                ReceiptStatus::Accepted,
            ));
        }
        assert_eq!(store.len(), MEMORY_CAPACITY);
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

    #[test]
    fn jsonl_sink_appends_to_temp_file() {
        let tmpdir = tempfile::tempdir().expect("tmpdir");
        let path = tmpdir.path().join("nested/receipts.jsonl");
        let sink = JsonlReceiptSink::with_path(path.clone());
        let receipt = WebActionReceipt::new(
            "@alice",
            "open_logs",
            "action",
            "open_logs",
            Some("key-1".into()),
            Some("sha256:before".into()),
            Some("sha256:after".into()),
            RiskTier::Low,
            ReceiptStatus::Accepted,
        );
        sink.stamp(&receipt);
        let body = std::fs::read_to_string(&path).expect("file written");
        assert!(body.contains(&receipt.id));
        assert!(body.contains("open_logs"));
        assert!(body.ends_with('\n'));
    }
}
