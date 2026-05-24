//! Owner: db-boundary / external Jekko LLM key pool
//! Proof: `cargo test -p jeryu --lib db::jekko_llm_pool_repo`
//!
//! Typed access to the per-user Jekko `state.sqlite` files consumed by the
//! JeRyu LLM balancer. SQL stays here so the LLM router only sees typed key
//! usage records and never imports `sqlx::` directly.

use std::path::Path;

use anyhow::{Context, Result};

use crate::db::{AnyPool, AnyPoolOptions, Row, install_default_drivers, raw_query};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUsage {
    pub provider: String,
    pub model: String,
    pub attempts: u64,
    pub failures: u64,
    pub last_failure_at: Option<i64>,
    pub cooldown_until: Option<i64>,
    pub status: String,
}

impl KeyUsage {
    pub fn ready(provider: &str, model: &str) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            attempts: 0,
            failures: 0,
            last_failure_at: None,
            cooldown_until: None,
            status: "ready".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureUpdate {
    pub status: String,
    pub failed_at: i64,
    pub cooldown_until: Option<i64>,
}

async fn open_pool(path: &Path) -> Result<AnyPool> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    install_default_drivers();
    let url = crate::db::config::sqlite_url(path);
    AnyPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .with_context(|| format!("connect {}", path.display()))
}

async fn ensure_schema(pool: &AnyPool) -> Result<()> {
    raw_query(
        "CREATE TABLE IF NOT EXISTS key_usage (
            provider        TEXT NOT NULL,
            model           TEXT NOT NULL,
            attempts        INTEGER NOT NULL DEFAULT 0,
            failures        INTEGER NOT NULL DEFAULT 0,
            last_failure_at INTEGER,
            cooldown_until  INTEGER,
            status          TEXT NOT NULL DEFAULT 'ready',
            PRIMARY KEY (provider, model)
        )",
    )
    .execute(pool)
    .await
    .context("create key_usage schema")?;
    Ok(())
}

pub async fn ensure_key_usage_schema(path: &Path) -> Result<()> {
    let pool = open_pool(path).await?;
    ensure_schema(&pool).await
}

pub async fn load_key_usage(path: &Path, provider: &str, model: &str) -> Result<KeyUsage> {
    let pool = open_pool(path).await?;
    ensure_schema(&pool).await?;
    let row = raw_query(
        "SELECT provider, model, attempts, failures, last_failure_at, cooldown_until, status
         FROM key_usage
         WHERE provider = ? AND model = ?",
    )
    .bind(provider)
    .bind(model)
    .fetch_optional(&pool)
    .await
    .context("load key_usage row")?;
    let Some(row) = row else {
        return Ok(KeyUsage::ready(provider, model));
    };
    let attempts: i64 = row.try_get("attempts").unwrap_or(0);
    let failures: i64 = row.try_get("failures").unwrap_or(0);
    Ok(KeyUsage {
        provider: row
            .try_get::<String, _>("provider")
            .unwrap_or_else(|_| provider.to_string()),
        model: row
            .try_get::<String, _>("model")
            .unwrap_or_else(|_| model.to_string()),
        attempts: attempts.max(0) as u64,
        failures: failures.max(0) as u64,
        last_failure_at: row.try_get("last_failure_at").ok(),
        cooldown_until: row.try_get("cooldown_until").ok(),
        status: row
            .try_get::<String, _>("status")
            .unwrap_or_else(|_| "ready".to_string()),
    })
}

pub async fn record_key_success(path: &Path, provider: &str, model: &str) -> Result<()> {
    let pool = open_pool(path).await?;
    ensure_schema(&pool).await?;
    raw_query(
        "INSERT INTO key_usage
             (provider, model, attempts, failures, last_failure_at, cooldown_until, status)
         VALUES (?, ?, 1, 0, NULL, NULL, 'ready')
         ON CONFLICT(provider, model) DO UPDATE SET
             attempts = attempts + 1,
             failures = 0,
             last_failure_at = NULL,
             cooldown_until = NULL,
             status = 'ready'",
    )
    .bind(provider)
    .bind(model)
    .execute(&pool)
    .await
    .context("record key_usage success")?;
    Ok(())
}

pub async fn record_key_failure(
    path: &Path,
    provider: &str,
    model: &str,
    failure: &FailureUpdate,
) -> Result<()> {
    let pool = open_pool(path).await?;
    ensure_schema(&pool).await?;
    raw_query(
        "INSERT INTO key_usage
             (provider, model, attempts, failures, last_failure_at, cooldown_until, status)
         VALUES (?, ?, 1, 1, ?, ?, ?)
         ON CONFLICT(provider, model) DO UPDATE SET
             attempts = attempts + 1,
             failures = failures + 1,
             last_failure_at = excluded.last_failure_at,
             cooldown_until = excluded.cooldown_until,
             status = excluded.status",
    )
    .bind(provider)
    .bind(model)
    .bind(failure.failed_at)
    .bind(failure.cooldown_until)
    .bind(failure.status.as_str())
    .execute(&pool)
    .await
    .context("record key_usage failure")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_schema_and_roundtrips_success_failure() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.sqlite");

        ensure_key_usage_schema(&state).await.unwrap();
        let initial = load_key_usage(&state, "openrouter", "model-a")
            .await
            .unwrap();
        assert_eq!(initial.status, "ready");
        assert_eq!(initial.attempts, 0);

        record_key_failure(
            &state,
            "openrouter",
            "model-a",
            &FailureUpdate {
                status: "rate_limited".to_string(),
                failed_at: 100,
                cooldown_until: Some(160),
            },
        )
        .await
        .unwrap();
        let failed = load_key_usage(&state, "openrouter", "model-a")
            .await
            .unwrap();
        assert_eq!(failed.status, "rate_limited");
        assert_eq!(failed.attempts, 1);
        assert_eq!(failed.failures, 1);
        assert_eq!(failed.cooldown_until, Some(160));

        record_key_success(&state, "openrouter", "model-a")
            .await
            .unwrap();
        let ready = load_key_usage(&state, "openrouter", "model-a")
            .await
            .unwrap();
        assert_eq!(ready.status, "ready");
        assert_eq!(ready.attempts, 2);
        assert_eq!(ready.failures, 0);
        assert_eq!(ready.cooldown_until, None);
    }
}
