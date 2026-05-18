//! Owner: Release Pipeline / Foundry Train (Wave 3.5.B)
//! Proof: `cargo test -p jeryu --lib release::sql_foundry_queue`
//! Invariants:
//!   - `enqueue` is idempotent on `candidate.id` via `INSERT OR IGNORE`.
//!     A crash between enqueue and drain leaves the candidate persisted;
//!     a retried enqueue after restart is a no-op.
//!   - `drain_ready` mirrors the in-memory `FoundryTrain::drain_ready`
//!     trigger semantics exactly:
//!       1. `split_on_high_risk` → if the FIFO-head candidate's own
//!          `commits.len() > max_commits`, ship it solo.
//!       2. otherwise drain ALL pending candidates iff EITHER
//!          (a) summed `commits.len() >= max_commits`, OR
//!          (b) the oldest pending candidate has waited
//!              `>= max_wait_minutes` since `created_at`.
//!          A drain "removes" candidates by setting `drained_at = now` — the
//!          row is preserved for audit but excluded from future `drain_ready`
//!          and `peek_pending` calls (lifecycle update, not append-only).
//!   - `commits_json` is the source of truth for the candidate's commit
//!     list; per-column fields exist only for indexed queries.
//!
//! Wave 11.A: SQL queries moved to `src/db/release_repo.rs`. This file
//! holds the `FoundryQueue` trait impl, which now delegates to
//! `ReleaseRepo`. Public API is unchanged.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::db::AnyPool;
use crate::db::release_repo::ReleaseRepo;

use super::foundry::{FoundryConfig, FoundryQueue, ReleaseCandidate};

/// SQL-backed `FoundryQueue`. Backs the in-memory `FoundryTrain` for
/// production use so queued release candidates survive a control-plane
/// restart. Mirrors `SqlVerdictStore` (Wave 7.A) in style.
#[derive(Debug, Clone)]
pub struct SqlFoundryQueue {
    repo: ReleaseRepo,
    config: FoundryConfig,
}

impl SqlFoundryQueue {
    pub fn new(pool: AnyPool, config: FoundryConfig) -> Self {
        Self {
            repo: ReleaseRepo::new(pool),
            config,
        }
    }
}

#[async_trait]
impl FoundryQueue for SqlFoundryQueue {
    async fn enqueue(&self, candidate: ReleaseCandidate) -> Result<()> {
        self.repo.foundry_enqueue(&candidate).await
    }

    async fn drain_ready(&self, now: DateTime<Utc>) -> Result<Vec<ReleaseCandidate>> {
        self.repo.foundry_drain_ready(now, &self.config).await
    }

    async fn peek_pending(&self) -> Result<usize> {
        self.repo.foundry_peek_pending().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::release_repo::{fresh_release_pool, fresh_release_pool_shared};
    use crate::release::foundry::FoundryTrain;
    use chrono::{Duration, TimeZone};

    async fn fresh_db() -> AnyPool {
        // Test fixture moved to the db boundary so this file no longer
        // imports `sqlx::` (closes HLT-006).
        fresh_release_pool().await
    }

    async fn fresh_db_shared() -> AnyPool {
        fresh_release_pool_shared().await
    }

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

    #[tokio::test]
    async fn enqueue_then_peek_pending_returns_one() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(10, 60, false));
        q.enqueue(cand("a", 1, fixed_now())).await.unwrap();
        assert_eq!(q.peek_pending().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn enqueue_is_idempotent_on_id() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(10, 60, false));
        let c = cand("dup", 1, fixed_now());
        q.enqueue(c.clone()).await.unwrap();
        q.enqueue(c.clone()).await.unwrap();
        q.enqueue(c).await.unwrap();
        assert_eq!(
            q.peek_pending().await.unwrap(),
            1,
            "same id must not insert twice"
        );
    }

    #[tokio::test]
    async fn drain_ready_returns_empty_for_empty_queue() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(1, 1, true));
        assert!(q.drain_ready(fixed_now()).await.unwrap().is_empty());
        assert!(
            q.drain_ready(fixed_now() + Duration::hours(24))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(q.peek_pending().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn drain_ready_returns_candidates_in_fifo_order() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(3, 60, false));
        let t0 = fixed_now();
        // Stagger created_at so ORDER BY created_at ASC is deterministic.
        q.enqueue(cand("a", 1, t0)).await.unwrap();
        q.enqueue(cand("b", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        q.enqueue(cand("c", 1, t0 + Duration::seconds(2)))
            .await
            .unwrap();
        // total_commits = 3 >= max_commits → drain all.
        let drained = q.drain_ready(t0 + Duration::seconds(3)).await.unwrap();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].id, "a");
        assert_eq!(drained[1].id, "b");
        assert_eq!(drained[2].id, "c");
    }

    #[tokio::test]
    async fn drain_ready_marks_candidates_drained_at() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(1, 60, false));
        let t0 = fixed_now();
        q.enqueue(cand("solo", 1, t0)).await.unwrap();
        let drained = q.drain_ready(t0).await.unwrap();
        assert_eq!(drained.len(), 1);
        // After drain the count goes to 0; the row is still present but
        // drained_at is set.
        assert_eq!(q.peek_pending().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn drained_candidates_not_returned_again() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(1, 60, false));
        let t0 = fixed_now();
        q.enqueue(cand("once", 1, t0)).await.unwrap();
        let first = q.drain_ready(t0).await.unwrap();
        assert_eq!(first.len(), 1);
        // Second drain on the same data must be empty.
        let second = q.drain_ready(t0 + Duration::seconds(1)).await.unwrap();
        assert!(second.is_empty(), "drained candidates must not re-emit");
    }

    #[tokio::test]
    async fn peek_pending_excludes_drained_candidates() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(1, 60, false));
        let t0 = fixed_now();
        q.enqueue(cand("x", 1, t0)).await.unwrap();
        q.enqueue(cand("y", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        assert_eq!(q.peek_pending().await.unwrap(), 2);
        let _ = q.drain_ready(t0 + Duration::seconds(2)).await.unwrap();
        assert_eq!(
            q.peek_pending().await.unwrap(),
            0,
            "drained rows must drop out of peek_pending"
        );
    }

    #[tokio::test]
    async fn drain_ready_respects_max_wait_minutes_threshold() {
        // Single candidate, well under max_commits. Only the wait trigger
        // can fire.
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(100, 10, false));
        let t0 = fixed_now();
        q.enqueue(cand("fresh", 1, t0)).await.unwrap();
        // At t0, candidate is too fresh — no drain.
        assert!(q.drain_ready(t0).await.unwrap().is_empty());
        // At t0+5min, still under the 10min threshold — no drain.
        assert!(
            q.drain_ready(t0 + Duration::minutes(5))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(q.peek_pending().await.unwrap(), 1);
        // At t0+11min, threshold met — drain fires.
        let drained = q.drain_ready(t0 + Duration::minutes(11)).await.unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "fresh");
    }

    #[tokio::test]
    async fn drain_ready_respects_max_commits_threshold() {
        // max_wait is 999 minutes so the wait trigger cannot fire in this
        // test. Only the commit-count trigger can.
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(3, 999, false));
        let t0 = fixed_now();
        q.enqueue(cand("small1", 1, t0)).await.unwrap();
        assert!(q.drain_ready(t0).await.unwrap().is_empty(), "1 < 3");
        q.enqueue(cand("small2", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        assert!(q.drain_ready(t0).await.unwrap().is_empty(), "2 < 3");
        q.enqueue(cand("small3", 1, t0 + Duration::seconds(2)))
            .await
            .unwrap();
        // 3 >= 3 → drain fires and ships all three.
        let drained = q.drain_ready(t0).await.unwrap();
        assert_eq!(drained.len(), 3);
    }

    #[tokio::test]
    async fn restart_survival_round_trip() {
        // Persist via one SqlFoundryQueue instance, then reconstruct a
        // second one over the same pool — pending candidates must survive.
        let pool = fresh_db().await;
        let config = cfg(10, 60, false);
        let q1 = SqlFoundryQueue::new(pool.clone(), config);
        q1.enqueue(cand("survivor-1", 1, fixed_now()))
            .await
            .unwrap();
        q1.enqueue(cand("survivor-2", 1, fixed_now()))
            .await
            .unwrap();
        drop(q1);
        let q2 = SqlFoundryQueue::new(pool, config);
        assert_eq!(
            q2.peek_pending().await.unwrap(),
            2,
            "candidates must survive a fresh queue handle on the same pool"
        );
        let drained = q2
            .drain_ready(fixed_now() + Duration::hours(2))
            .await
            .unwrap();
        assert_eq!(drained.len(), 2);
        let ids: Vec<&str> = drained.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"survivor-1"));
        assert!(ids.contains(&"survivor-2"));
    }

    #[tokio::test]
    async fn concurrent_enqueue_no_corruption_with_four_tasks() {
        let q = SqlFoundryQueue::new(fresh_db_shared().await, cfg(100, 60, false));
        let mut handles = Vec::new();
        for task in 0..4 {
            let q = q.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..5 {
                    let id = format!("t{task}-i{i}");
                    let c = cand(
                        &id,
                        1,
                        fixed_now() + Duration::milliseconds((task * 100 + i) as i64),
                    );
                    q.enqueue(c).await.expect("concurrent enqueue");
                }
            }));
        }
        for h in handles {
            h.await.expect("task joined");
        }
        assert_eq!(
            q.peek_pending().await.unwrap(),
            20,
            "4 tasks * 5 distinct ids must produce 20 pending rows"
        );
    }

    #[tokio::test]
    async fn commits_json_round_trips_complex_payload() {
        let q = SqlFoundryQueue::new(fresh_db().await, cfg(100, 60, false));
        let t0 = fixed_now();
        let complex = ReleaseCandidate {
            id: "complex-1".into(),
            commits: vec![
                "deadbeefcafef00d".repeat(2),
                "0000000000000000000000000000000000000001".into(),
                // Commit messages with quotes/newlines/unicode would never
                // be raw SHAs in practice, but stress JSON escaping anyway.
                "weird:\"quoted\"\nand\u{1F600}".into(),
            ],
            source_branch: "feat/complex unicode\u{1F680}".into(),
            head_sha: "f".repeat(40),
            created_at: t0,
        };
        q.enqueue(complex.clone()).await.unwrap();
        let drained = q.drain_ready(t0 + Duration::hours(2)).await.unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0], complex,
            "complex payload must round-trip losslessly"
        );
    }

    /// Wave 10 — `cmd_foundry` flipped from the in-memory `FoundryTrain`
    /// to this `SqlFoundryQueue`. This test asserts the round-trip the
    /// CLI relies on: enqueue a candidate, drain on the same tick (the
    /// CLI is one-shot), and confirm the SAME candidate comes back out
    /// (id, head_sha, source_branch, commits all preserved). If the
    /// CLI accidentally falls back to the in-memory queue, this test
    /// keeps passing — but the companion `_type_check` in
    /// `src/bin/autonomy.rs` pins the type.
    #[tokio::test]
    async fn cli_path_uses_sql_queue_when_pool_present() {
        let pool = fresh_db().await;
        // CLI uses `FoundryConfig::default()` (Wave 10 mint); mirror that
        // here so any future change to the default is caught.
        let q = SqlFoundryQueue::new(pool, FoundryConfig::default());
        let t0 = fixed_now();
        let original = cand("cli-run-7", 1, t0);
        q.enqueue(original.clone()).await.unwrap();

        // The CLI calls `drain_ready(now + 1 day)` to force the wait
        // trigger to fire on a one-shot invocation. Mirror that here.
        let drained = q
            .drain_ready(t0 + Duration::days(1))
            .await
            .expect("drain after CLI enqueue");
        assert_eq!(
            drained.len(),
            1,
            "single CLI enqueue must produce exactly one drained candidate"
        );
        assert_eq!(drained[0], original, "candidate must round-trip losslessly");
        // And the queue must mark it drained — a second drain returns empty.
        let second = q
            .drain_ready(t0 + Duration::days(2))
            .await
            .expect("second drain");
        assert!(
            second.is_empty(),
            "drained candidate must not re-emit on the next tick"
        );
        assert_eq!(q.peek_pending().await.unwrap(), 0);
    }

    // Bonus parity test: in-memory FoundryTrain and SqlFoundryQueue must
    // agree on `peek_pending()` for the same enqueue/drain sequence under
    // identical FoundryConfig.
    #[tokio::test]
    async fn in_memory_and_sql_impls_agree_on_simple_sequence() {
        let config = cfg(3, 60, false);
        let mem = FoundryTrain::new(config);
        let sql = SqlFoundryQueue::new(fresh_db().await, config);
        let t0 = fixed_now();

        // Enqueue 2 → both report 2 pending; under-threshold drain returns empty.
        FoundryQueue::enqueue(&mem, cand("a", 1, t0)).await.unwrap();
        sql.enqueue(cand("a", 1, t0)).await.unwrap();
        FoundryQueue::enqueue(&mem, cand("b", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        sql.enqueue(cand("b", 1, t0 + Duration::seconds(1)))
            .await
            .unwrap();
        assert_eq!(FoundryQueue::peek_pending(&mem).await.unwrap(), 2);
        assert_eq!(sql.peek_pending().await.unwrap(), 2);
        // Disambiguate against the inherent `drain_ready` on `FoundryTrain`,
        // which is sync and would otherwise shadow the async trait method.
        assert!(
            FoundryQueue::drain_ready(&mem, t0)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(sql.drain_ready(t0).await.unwrap().is_empty());

        // Add a third → both should drain all three (commit trigger).
        FoundryQueue::enqueue(&mem, cand("c", 1, t0 + Duration::seconds(2)))
            .await
            .unwrap();
        sql.enqueue(cand("c", 1, t0 + Duration::seconds(2)))
            .await
            .unwrap();
        let mem_drained = FoundryQueue::drain_ready(&mem, t0 + Duration::seconds(3))
            .await
            .unwrap();
        let sql_drained = sql.drain_ready(t0 + Duration::seconds(3)).await.unwrap();
        assert_eq!(mem_drained.len(), 3);
        assert_eq!(sql_drained.len(), 3);
        assert_eq!(FoundryQueue::peek_pending(&mem).await.unwrap(), 0);
        assert_eq!(sql.peek_pending().await.unwrap(), 0);
    }
}
