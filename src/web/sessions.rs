//! Opaque-token session store backing the JeRyu Web Forge BFF.
//!
//! `__Host-jeryu-session=<32-byte hex>` is the session cookie issued by
//! `POST /api/v1/auth/login`. The token itself is a random 32-byte (64
//! hex char) opaque identifier — the BFF never embeds claims in the
//! cookie value, so revocation is unconditional: deleting the row from
//! `sessions` is sufficient to invalidate every browser holding the
//! cookie.
//!
//! Schema: see `db/state.rs::migrate` (the inline DDL string that
//! includes the `sessions` table) and `db/migrations/202606010001_web_forge_core.sql`
//! (documentation/canonical mirror).
//!
//! `last_seen_at` is bumped at most once per minute per session to keep
//! the write rate cheap on every authenticated request. The in-memory
//! LRU on the `SessionStore` deduplicates touches without round-tripping
//! the DB. Persisted state still reflects the bump at the next
//! once-per-minute window.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rand::RngCore;
use sqlx::{AnyPool, Row};

/// Minimum gap between successive `last_seen_at` updates for the same
/// session. The full session window is 30 days; updating last-seen on
/// every request would be a hot row write. Once-per-minute is the
/// canonical contract from `WEB_WORK_CLAUDE.md` §22.3.
pub const TOUCH_MIN_INTERVAL: Duration = Duration::from_secs(60);

/// One row from the `sessions` table, hydrated for the auth middleware.
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub actor_id: String,
    pub actor_login: String,
    pub perms: HashSet<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Backing store for opaque session tokens. Cheap to clone — the inner
/// pool + LRU are `Arc<...>`.
#[derive(Clone)]
pub struct SessionStore {
    pool: AnyPool,
    touch_cache: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
}

impl SessionStore {
    pub fn new(pool: AnyPool) -> Self {
        Self {
            pool,
            touch_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Generate a fresh 32-byte (64-hex-char) opaque session id.
    fn new_session_id() -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Insert a new session row and return the hydrated record. Caller
    /// supplies the perm set (already filtered to canonical keys). `ttl`
    /// is added to `now()` to form `expires_at`.
    pub async fn create(
        &self,
        actor_id: &str,
        actor_login: &str,
        perms: HashSet<String>,
        ttl: Duration,
    ) -> Result<SessionRecord> {
        let id = Self::new_session_id();
        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::from_std(ttl)
                .context("session ttl out of range for chrono::Duration")?;
        let perms_json = serde_json::to_string(&perms.iter().cloned().collect::<Vec<String>>())
            .context("serialize session perms")?;

        sqlx::query(
            "INSERT INTO sessions
                 (id, actor_id, actor_login, perms_json,
                  created_at, expires_at, last_seen_at, user_agent, ip)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(&id)
        .bind(actor_id)
        .bind(actor_login)
        .bind(&perms_json)
        .bind(now.to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert sessions row")?;

        Ok(SessionRecord {
            id,
            actor_id: actor_id.to_string(),
            actor_login: actor_login.to_string(),
            perms,
            created_at: now,
            expires_at,
            last_seen_at: now,
        })
    }

    /// Look up a session by id. Returns `None` if the row is missing OR
    /// `expires_at < now()`.
    pub async fn find(&self, id: &str) -> Result<Option<SessionRecord>> {
        let row = sqlx::query(
            "SELECT id, actor_id, actor_login, perms_json,
                    created_at, expires_at, last_seen_at
             FROM sessions
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("select sessions row")?;

        let Some(row) = row else { return Ok(None) };

        let id: String = row.try_get("id").context("read session.id")?;
        let actor_id: String = row.try_get("actor_id").context("read session.actor_id")?;
        let actor_login: String = row
            .try_get("actor_login")
            .context("read session.actor_login")?;
        let perms_json: String = row.try_get("perms_json").context("read session.perms_json")?;
        let created_at_s: String = row.try_get("created_at").context("read session.created_at")?;
        let expires_at_s: String = row.try_get("expires_at").context("read session.expires_at")?;
        let last_seen_at_s: String = row
            .try_get("last_seen_at")
            .context("read session.last_seen_at")?;

        let created_at = DateTime::parse_from_rfc3339(&created_at_s)
            .context("parse session.created_at")?
            .with_timezone(&Utc);
        let expires_at = DateTime::parse_from_rfc3339(&expires_at_s)
            .context("parse session.expires_at")?
            .with_timezone(&Utc);
        let last_seen_at = DateTime::parse_from_rfc3339(&last_seen_at_s)
            .context("parse session.last_seen_at")?
            .with_timezone(&Utc);

        if expires_at <= Utc::now() {
            return Ok(None);
        }

        let perms_vec: Vec<String> =
            serde_json::from_str(&perms_json).context("parse session.perms_json")?;
        let perms = perms_vec.into_iter().collect::<HashSet<String>>();

        Ok(Some(SessionRecord {
            id,
            actor_id,
            actor_login,
            perms,
            created_at,
            expires_at,
            last_seen_at,
        }))
    }

    /// Update `last_seen_at` to `now()` for the supplied session, but at
    /// most once per [`TOUCH_MIN_INTERVAL`] per session. The first touch
    /// after process start always writes; subsequent calls inside the
    /// window are no-ops.
    pub async fn touch(&self, id: &str) -> Result<()> {
        let now = Utc::now();
        {
            let mut cache = self.touch_cache.lock();
            if let Some(last) = cache.get(id)
                && now.signed_duration_since(*last)
                    < chrono::Duration::from_std(TOUCH_MIN_INTERVAL)
                        .expect("touch interval fits in chrono::Duration")
            {
                return Ok(());
            }
            cache.insert(id.to_string(), now);
        }
        sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ?")
            .bind(now.to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await
            .context("update sessions.last_seen_at")?;
        Ok(())
    }

    /// Delete the session row, invalidating every browser holding the
    /// cookie. The accompanying touch-cache entry is also purged so a
    /// re-issued session with the same id (extremely unlikely; 32 bytes
    /// of entropy) does not inherit a stale touch timestamp.
    pub async fn revoke(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("delete sessions row")?;
        self.touch_cache.lock().remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu::state::Db;

    async fn fresh_store() -> SessionStore {
        let db = Db::open_memory().await.expect("open in-memory state Db");
        SessionStore::new(db.pool())
    }

    #[tokio::test]
    async fn create_then_find_roundtrips() {
        let store = fresh_store().await;
        let perms: HashSet<String> = ["repo.read".into(), "repo.write".into()].into();
        let rec = store
            .create("u-1", "alice", perms.clone(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(rec.id.len(), 64, "32 bytes hex = 64 chars");
        let loaded = store.find(&rec.id).await.unwrap().expect("session present");
        assert_eq!(loaded.actor_login, "alice");
        assert_eq!(loaded.perms, perms);
    }

    #[tokio::test]
    async fn find_returns_none_for_unknown_id() {
        let store = fresh_store().await;
        assert!(store.find("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn find_returns_none_for_expired_session() {
        let store = fresh_store().await;
        // ttl=0 → expires_at == created_at, immediately stale.
        let rec = store
            .create("u-1", "alice", HashSet::new(), Duration::from_millis(0))
            .await
            .unwrap();
        // Beat any clock skew by sleeping 5ms.
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(store.find(&rec.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn revoke_removes_session() {
        let store = fresh_store().await;
        let rec = store
            .create("u-1", "alice", HashSet::new(), Duration::from_secs(60))
            .await
            .unwrap();
        store.revoke(&rec.id).await.unwrap();
        assert!(store.find(&rec.id).await.unwrap().is_none());
    }
}
