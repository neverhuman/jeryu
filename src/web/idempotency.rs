//! In-memory idempotency store (W-CC-06).
//!
//! Phase-1 keeps the cache process-local — `parking_lot::Mutex<HashMap>`
//! is fine for a single BFF instance. Future work (W-F-05 migration +
//! W-B-* services) backs the same trait surface with the
//! `web_action_receipts` SQLite table whose `UNIQUE(action_kind,
//! target_id, idempotency_key)` constraint enforces single-execution at
//! the durable layer (see §35.1.16).

use std::collections::HashMap;

use parking_lot::Mutex;
use serde_json::Value;

/// Process-local replay cache. Phase-1 expects callers to gate the lookup
/// behind a fully-qualified key (action_kind + target + raw idempotency
/// header value); the store itself is opaque.
pub struct IdempotencyStore {
    cache: Mutex<HashMap<String, Value>>,
}

impl IdempotencyStore {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Return the cached result for `key`, or `None` if no replay record
    /// exists.
    pub fn find(&self, key: &str) -> Option<Value> {
        self.cache.lock().get(key).cloned()
    }

    /// Store the result `value` against `key`. Returns the previous value
    /// (used by services to detect `idempotency_conflict` when a partial
    /// retry crosses a different payload).
    pub fn store(&self, key: String, value: Value) -> Option<Value> {
        self.cache.lock().insert(key, value)
    }

    /// Number of cached entries — exposed for diagnostics.
    pub fn len(&self) -> usize {
        self.cache.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.lock().is_empty()
    }
}

impl Default for IdempotencyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_find_roundtrip() {
        let s = IdempotencyStore::new();
        assert!(s.is_empty());
        assert!(s.find("k").is_none());
        let prev = s.store("k".into(), serde_json::json!({"ok": true}));
        assert!(prev.is_none());
        assert_eq!(s.len(), 1);
        let cached = s.find("k").expect("cached value");
        assert_eq!(cached.get("ok").and_then(|v| v.as_bool()), Some(true));
    }

    #[test]
    fn store_returns_previous_on_conflict() {
        let s = IdempotencyStore::new();
        s.store("k".into(), serde_json::json!({"v": 1}));
        let prev = s.store("k".into(), serde_json::json!({"v": 2}));
        let prev = prev.expect("previous value returned");
        assert_eq!(prev.get("v").and_then(|v| v.as_i64()), Some(1));
    }
}
