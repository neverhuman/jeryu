//! Audit-event writer (W-CC-05 / §35.1.16).
//!
//! Every mutating handler calls [`write_audit`] which appends a row to
//! the `audit_events` SQLite table. The row carries actor, action id,
//! target entity, risk tier, preview JSON (if available), result JSON,
//! and `created_at`. A structured tracing event is emitted alongside the
//! INSERT so log-only consumers (jankurai, jeryu-ops) still get the
//! event.
//!
//! Schema: see `db/state.rs::migrate` (inline DDL) and
//! `db/migrations/202606010001_web_forge_core.sql` (canonical mirror).

use chrono::Utc;
use serde_json::Value;
use sqlx::AnyPool;
use tracing::info;
use uuid::Uuid;

/// Risk tiers used by the §35.1.14 action-safety pipeline. `Low` actions
/// (read-only / metadata) skip preview; `Medium` actions show a preview;
/// `High` actions additionally require expected-SHA / expected-hash
/// concurrency tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskTier {
    Low,
    Medium,
    High,
}

impl RiskTier {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Default `actor_kind` for the rest-handler call sites. Most BFF
/// mutations originate from a browser session whose actor is a human
/// login; agent-initiated audit rows can use the lower-level
/// [`write_audit_with_kind`] variant.
const HUMAN_ACTOR_KIND: &str = "user";

/// Persist an audit row keyed by `(actor_kind="user", actor_id, action_id,
/// target_entity)`. Convenience wrapper over [`write_audit_with_kind`] for
/// the human-driven REST surface.
pub async fn write_audit(
    pool: &AnyPool,
    actor: &str,
    action_id: &str,
    target_entity: &str,
    risk_tier: RiskTier,
    payload: Value,
) -> anyhow::Result<String> {
    write_audit_with_kind(
        pool,
        HUMAN_ACTOR_KIND,
        actor,
        action_id,
        target_entity,
        risk_tier,
        Some(&payload),
        None,
    )
    .await
}

/// Persist an audit row with explicit `actor_kind` and optional
/// preview/result JSON payloads. Returns the freshly minted UUID so the
/// caller can plumb it through to event payloads / response shapes.
pub async fn write_audit_with_kind(
    pool: &AnyPool,
    actor_kind: &str,
    actor_id: &str,
    action_id: &str,
    target_entity: &str,
    risk_tier: RiskTier,
    preview_json: Option<&Value>,
    result_json: Option<&Value>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let created_at = Utc::now().to_rfc3339();
    let preview_str = preview_json.map(|v| v.to_string());
    let result_str = result_json.map(|v| v.to_string());

    sqlx::query(
        "INSERT INTO audit_events
            (id, actor_kind, actor_id, action_id, target_entity, risk_tier,
             preview_json, result_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(action_id)
    .bind(target_entity)
    .bind(risk_tier.label())
    .bind(preview_str.as_deref())
    .bind(result_str.as_deref())
    .bind(&created_at)
    .execute(pool)
    .await?;

    info!(
        target: "jeryu.web.audit",
        audit_id = %id,
        actor = %actor_id,
        action = %action_id,
        audit_target = %target_entity,
        risk_tier = risk_tier.label(),
        "audit event written"
    );
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu::state::Db;
    use sqlx::Row;

    #[test]
    fn risk_tier_labels_match_spec() {
        assert_eq!(RiskTier::Low.label(), "low");
        assert_eq!(RiskTier::Medium.label(), "medium");
        assert_eq!(RiskTier::High.label(), "high");
    }

    #[tokio::test]
    async fn write_audit_inserts_one_row() {
        let db = Db::open_memory().await.expect("open mem db");
        let pool = db.pool();
        let id = write_audit(
            &pool,
            "@alice",
            "mr.approve",
            "mr:gitlab/o/r/42",
            RiskTier::High,
            serde_json::json!({"head_sha": "deadbeef"}),
        )
        .await
        .expect("audit insert");
        assert!(!id.is_empty());

        let row = sqlx::query(
            "SELECT actor_kind, actor_id, action_id, target_entity, risk_tier, preview_json
             FROM audit_events WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("read back audit row");

        let actor_kind: String = row.try_get("actor_kind").unwrap();
        let actor_id: String = row.try_get("actor_id").unwrap();
        let action_id: String = row.try_get("action_id").unwrap();
        let target_entity: String = row.try_get("target_entity").unwrap();
        let risk_tier: String = row.try_get("risk_tier").unwrap();
        let preview_json: Option<String> = row.try_get("preview_json").unwrap();
        assert_eq!(actor_kind, "user");
        assert_eq!(actor_id, "@alice");
        assert_eq!(action_id, "mr.approve");
        assert_eq!(target_entity, "mr:gitlab/o/r/42");
        assert_eq!(risk_tier, "high");
        assert!(
            preview_json.as_deref().unwrap_or("").contains("deadbeef"),
            "preview_json must include the payload body"
        );
    }
}
