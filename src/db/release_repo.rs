//! Owner: db-boundary (Wave 11.A)
//! Proof: `cargo test -p jeryu --lib -- db::release_repo`
//!
//! Typed repo for the foundry-train queue. Closes
//! `HLT-006-DIRECT-DB-WRONG-LAYER` for the release side: the
//! `SqlFoundryQueue` wrapper no longer imports `sqlx::`.
//!
//! Invariants preserved (mirror of `SqlFoundryQueue`):
//!   - `foundry_enqueue` is idempotent on `candidate.id` via
//!     `INSERT OR IGNORE`.
//!   - `foundry_drain_ready` mirrors `FoundryTrain::drain_ready`:
//!       1. split-on-high-risk → head solo when its own commits
//!          exceed `max_commits`.
//!       2. otherwise drain every pending row iff the summed commits
//!          hit `max_commits` OR the oldest waited past
//!          `max_wait_minutes`.
//!   - Drained rows are stamped `drained_at = now` (lifecycle update,
//!     not append-only); subsequent calls exclude them.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::AnyPool;
use sqlx::Row;
use sqlx::any::AnyRow;

use crate::release::foundry::{FoundryConfig, ReleaseCandidate};

/// Typed repo for the `foundry_candidates` table.
#[derive(Debug, Clone)]
pub struct ReleaseRepo {
    pool: AnyPool,
}

impl ReleaseRepo {
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &AnyPool {
        &self.pool
    }

    /// Insert a candidate row. Idempotent on `candidate.id`.
    pub async fn foundry_enqueue(&self, candidate: &ReleaseCandidate) -> Result<()> {
        let commits_json = serde_json::to_string(&candidate.commits)
            .context("serialize candidate commits to commits_json")?;
        sqlx::query(
            "INSERT OR IGNORE INTO foundry_candidates
                 (id, head_sha, source_branch, commits_json, created_at, drained_at)
             VALUES (?, ?, ?, ?, ?, NULL)",
        )
        .bind(&candidate.id)
        .bind(&candidate.head_sha)
        .bind(&candidate.source_branch)
        .bind(&commits_json)
        .bind(candidate.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("insert foundry_candidates row")?;
        Ok(())
    }

    /// Drain rows that should ship now. The decision is identical to
    /// the in-memory `FoundryTrain::drain_ready` so the SQL queue and
    /// the in-memory queue behave the same.
    pub async fn foundry_drain_ready(
        &self,
        now: DateTime<Utc>,
        config: &FoundryConfig,
    ) -> Result<Vec<ReleaseCandidate>> {
        let pending = self.load_pending().await?;
        if pending.is_empty() {
            return Ok(Vec::new());
        }

        if config.split_on_high_risk {
            let head = &pending[0];
            if head.commits.len() > config.max_commits {
                let ids = vec![head.id.clone()];
                self.foundry_mark_drained(&ids, now).await?;
                return Ok(vec![head.clone()]);
            }
        }

        let total_commits: usize = pending.iter().map(|c| c.commits.len()).sum();
        let oldest_age = pending
            .first()
            .map(|c| now.signed_duration_since(c.created_at))
            .unwrap_or_else(Duration::zero);
        let wait_trigger = oldest_age >= Duration::minutes(config.max_wait_minutes);
        let commit_trigger = total_commits >= config.max_commits;

        if !wait_trigger && !commit_trigger {
            return Ok(Vec::new());
        }

        let ids: Vec<String> = pending.iter().map(|c| c.id.clone()).collect();
        self.foundry_mark_drained(&ids, now).await?;
        Ok(pending)
    }

    /// COUNT(*) of un-drained rows.
    pub async fn foundry_peek_pending(&self) -> Result<usize> {
        let row =
            sqlx::query("SELECT COUNT(*) AS n FROM foundry_candidates WHERE drained_at IS NULL")
                .fetch_one(&self.pool)
                .await
                .context("peek_pending count query")?;
        let n: i64 = row.try_get("n").context("read peek_pending count column")?;
        Ok(n.max(0) as usize)
    }

    /// Stamp `drained_at = now` on the given row ids. Bind each id
    /// explicitly so we don't hand-roll SQL escaping for an
    /// untrusted-feeling field.
    pub async fn foundry_mark_drained(&self, ids: &[String], now: DateTime<Utc>) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let now_str = now.to_rfc3339();
        for id in ids {
            sqlx::query(
                "UPDATE foundry_candidates
                 SET drained_at = ?
                 WHERE id = ? AND drained_at IS NULL",
            )
            .bind(&now_str)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("mark_drained update")?;
        }
        Ok(())
    }

    /// Read every un-drained row in FIFO order. Internal helper used
    /// by `foundry_drain_ready`.
    async fn load_pending(&self) -> Result<Vec<ReleaseCandidate>> {
        let rows = sqlx::query(
            "SELECT id, head_sha, source_branch, commits_json, created_at
             FROM foundry_candidates
             WHERE drained_at IS NULL
             ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("load_pending query")?;
        rows.iter().map(decode_candidate).collect()
    }
}

/// Decode a `foundry_candidates` row back into a `ReleaseCandidate`.
fn decode_candidate(row: &AnyRow) -> Result<ReleaseCandidate> {
    let id: String = row.try_get("id").context("read id")?;
    let head_sha: String = row.try_get("head_sha").context("read head_sha")?;
    let source_branch: String = row.try_get("source_branch").context("read source_branch")?;
    let commits_json: String = row.try_get("commits_json").context("read commits_json")?;
    let created_at_str: String = row.try_get("created_at").context("read created_at")?;
    let commits: Vec<String> =
        serde_json::from_str(&commits_json).context("decode commits_json")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .context("parse created_at rfc3339")?
        .with_timezone(&Utc);
    Ok(ReleaseCandidate {
        id,
        commits,
        source_branch,
        head_sha,
        created_at,
    })
}

// ---------------------------------------------------------------------------
// In-memory test schema installer.
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) async fn fresh_release_pool() -> AnyPool {
    use crate::db::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("redline::memory:")
        .await
        .expect("connect in-memory redline");
    for stmt in release_schema_ddl() {
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }
    pool
}

/// File-backed pool for the concurrent-write test. Each `redline::memory:`
/// connection has its own DB, so multi-connection tests use a tempfile path.
#[cfg(test)]
pub(crate) async fn fresh_release_pool_shared() -> AnyPool {
    use crate::db::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let tmp = tempfile::tempdir().expect("tempdir for shared release pool");
    let db_path = tmp.path().join("release.redline");
    let url = format!("redline:{}?mode=rwc", db_path.display());
    let pool = AnyPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect file-backed shared redline");
    std::mem::forget(tmp);
    let _ = sqlx::query("DROP TABLE IF EXISTS foundry_candidates")
        .execute(&pool)
        .await;
    for stmt in release_schema_ddl() {
        sqlx::query(stmt).execute(&pool).await.unwrap();
    }
    pool
}

#[cfg(test)]
pub(crate) fn release_schema_ddl() -> &'static [&'static str] {
    &[
        "CREATE TABLE foundry_candidates (
            id            TEXT PRIMARY KEY,
            head_sha      TEXT NOT NULL,
            source_branch TEXT NOT NULL,
            commits_json  TEXT NOT NULL,
            created_at    TEXT NOT NULL,
            drained_at    TEXT
        )",
        "CREATE INDEX idx_foundry_candidates_pending
             ON foundry_candidates(drained_at, created_at)",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cand(id: &str, commits: usize, at: DateTime<Utc>) -> ReleaseCandidate {
        ReleaseCandidate {
            id: id.into(),
            commits: (0..commits).map(|i| format!("{id}-c{i}")).collect(),
            source_branch: format!("feat/{id}"),
            head_sha: format!("{:0<40}", id),
            created_at: at,
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap()
    }

    fn cfg(max_commits: usize, max_wait_minutes: i64, split: bool) -> FoundryConfig {
        FoundryConfig {
            max_commits,
            max_wait_minutes,
            split_on_high_risk: split,
        }
    }

    // -- 1.
    #[tokio::test]
    async fn foundry_enqueue_then_peek_returns_one() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        repo.foundry_enqueue(&cand("a", 1, fixed_now()))
            .await
            .unwrap();
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 1);
    }

    // -- 2.
    #[tokio::test]
    async fn foundry_enqueue_is_idempotent_on_id() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let c = cand("dup", 1, fixed_now());
        repo.foundry_enqueue(&c).await.unwrap();
        repo.foundry_enqueue(&c).await.unwrap();
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 1);
    }

    // -- 3.
    #[tokio::test]
    async fn foundry_drain_ready_marks_rows() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let t0 = fixed_now();
        repo.foundry_enqueue(&cand("a", 1, t0)).await.unwrap();
        repo.foundry_enqueue(&cand("b", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        repo.foundry_enqueue(&cand("c", 1, t0 + Duration::seconds(2)))
            .await
            .unwrap();
        let drained = repo
            .foundry_drain_ready(t0 + Duration::seconds(3), &cfg(3, 60, false))
            .await
            .unwrap();
        assert_eq!(drained.len(), 3);
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 0);
    }

    // -- 4.
    #[tokio::test]
    async fn foundry_drain_returns_empty_for_empty_queue() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let drained = repo
            .foundry_drain_ready(fixed_now(), &cfg(1, 1, false))
            .await
            .unwrap();
        assert!(drained.is_empty());
    }

    // -- 5.
    #[tokio::test]
    async fn foundry_drain_returns_empty_under_thresholds() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let t0 = fixed_now();
        repo.foundry_enqueue(&cand("fresh", 1, t0)).await.unwrap();
        let drained = repo
            .foundry_drain_ready(t0, &cfg(10, 60, false))
            .await
            .unwrap();
        assert!(drained.is_empty());
    }

    // -- 6.
    #[tokio::test]
    async fn foundry_drain_respects_wait_trigger() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let t0 = fixed_now();
        repo.foundry_enqueue(&cand("waiter", 1, t0)).await.unwrap();
        // 5 minutes under a 10-minute wait threshold: no drain.
        let early = repo
            .foundry_drain_ready(t0 + Duration::minutes(5), &cfg(100, 10, false))
            .await
            .unwrap();
        assert!(early.is_empty());
        // 11 minutes past creation: wait trigger fires.
        let drained = repo
            .foundry_drain_ready(t0 + Duration::minutes(11), &cfg(100, 10, false))
            .await
            .unwrap();
        assert_eq!(drained.len(), 1);
    }

    // -- 7.
    #[tokio::test]
    async fn foundry_drain_split_on_high_risk_ships_head_solo() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let t0 = fixed_now();
        // Head candidate alone exceeds max_commits → ship it solo.
        repo.foundry_enqueue(&cand("big", 5, t0)).await.unwrap();
        repo.foundry_enqueue(&cand("small", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        let drained = repo
            .foundry_drain_ready(t0 + Duration::seconds(2), &cfg(3, 999, true))
            .await
            .unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "big");
    }

    // -- 8.
    #[tokio::test]
    async fn foundry_mark_drained_excludes_from_peek() {
        let repo = ReleaseRepo::new(fresh_release_pool().await);
        let t0 = fixed_now();
        repo.foundry_enqueue(&cand("x", 1, t0)).await.unwrap();
        repo.foundry_enqueue(&cand("y", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 2);
        repo.foundry_mark_drained(&["x".into(), "y".into()], t0)
            .await
            .unwrap();
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 0);
    }

    // -- 9.
    #[tokio::test]
    async fn foundry_concurrent_enqueue_no_corruption() {
        let repo = ReleaseRepo::new(fresh_release_pool_shared().await);
        let mut handles = Vec::new();
        for task in 0..4 {
            let repo = repo.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("t{task}-i{i}");
                    repo.foundry_enqueue(&cand(
                        &id,
                        1,
                        fixed_now() + Duration::milliseconds((task * 100 + i) as i64),
                    ))
                    .await
                    .expect("concurrent enqueue");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        assert_eq!(repo.foundry_peek_pending().await.unwrap(), 20);
    }
}
