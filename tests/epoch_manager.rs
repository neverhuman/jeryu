//! Integration coverage for `jeryu::epoch::EpochManager`.
//!
//! The DB plumbing (raw SQL, sqlx) lives in the `cache-brain-adapter` crate;
//! this test exercises the application-layer wrapper end to end against an
//! in-memory database and covers the epoch escalation contract documented on
//! `EpochManager`.

use cache_brain_adapter::AnyPool;
use jeryu::db::{AnyPoolOptions, install_default_drivers};
use jeryu::epoch::EpochManager;

async fn setup_db() -> AnyPool {
    install_default_drivers();
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect("redline::memory:")
        .await
        .unwrap();

    sqlx::query(
        "CREATE TABLE cache_epochs (
            scope TEXT PRIMARY KEY,
            current_epoch INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            author_job_id INTEGER NOT NULL,
            reason TEXT NOT NULL
        )",
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}

#[tokio::test]
async fn epoch_escalation_invalidates_old_objects() {
    let pool = setup_db().await;
    let mgr = EpochManager::new(pool);

    let scope = "project:42";

    // Initial epoch should be 0
    assert_eq!(mgr.get_epoch(scope).await.unwrap(), 0);
    assert!(mgr.is_valid(scope, 0).await.unwrap());

    // Bump epoch
    mgr.bump_epoch(scope, 999, "Security incident in base image")
        .await
        .unwrap();

    // New epoch
    assert_eq!(mgr.get_epoch(scope).await.unwrap(), 1);

    // Previous objects are now invalid
    assert!(!mgr.is_valid(scope, 0).await.unwrap());

    // New objects built today at epoch 1 are valid
    assert!(mgr.is_valid(scope, 1).await.unwrap());
}
