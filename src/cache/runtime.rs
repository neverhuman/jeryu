use super::*;

#[path = "runtime_cas.rs"]
mod cas;
#[path = "runtime_control.rs"]
mod control;
#[path = "runtime_gc.rs"]
mod gc;
pub use gc::sweep_incremental_caches;
#[path = "runtime_reports.rs"]
mod reports;

impl SmartCache {
    /// GC housekeeping: evict aged entries from the cache state layer.
    /// SmartCache owns the db field as a cache-boundary concern.
    pub(crate) async fn run_gc_housekeeping(&self, dry_run: bool) -> anyhow::Result<u64> {
        let freed = self.db.prune_cache_requests(7).await?;
        if !dry_run {
            let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
            let _ = self.db.prune_test_verdicts(&cutoff).await?;
            let _ = self.db.prune_action_cache(&cutoff).await?;
        }
        Ok(freed)
    }
}
