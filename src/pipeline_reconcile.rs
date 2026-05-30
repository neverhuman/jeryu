//! Owner: Pipeline-cache self-healing — reconcile tracked pipelines vs GitLab.
//! Proof: `cargo test -p jeryu --lib pipeline_reconcile`
//! Invariants:
//!   - Safety net for dropped pipeline webhooks. jeryu's `tracked_pipelines`
//!     cache is advanced by GitLab webhooks; if a webhook is lost (e.g. GitLab
//!     overloaded or restarting) a pipeline can stay non-terminal ("created")
//!     in the cache forever, so `jeryu next` and the TUI show phantom pipelines
//!     that no longer exist or already finished.
//!   - This loop periodically reconciles every non-terminal cached pipeline
//!     against its live GitLab status and writes the terminal status back. It
//!     also force-expires rows stuck far longer than any pipeline could run, so
//!     the cache converges even while GitLab is unreachable.
//!   - Tolerant: a GitLab outage leaves the cache intact and retries next tick;
//!     never deletes data, only advances a stale status to its true value.

use anyhow::Result;
use tracing::{debug, info};

use crate::engine::SharedState;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, TrackedPipeline};

/// GitLab pipeline statuses that are terminal (no further transitions).
const TERMINAL: &[&str] = &["success", "failed", "canceled", "skipped"];
/// Webhooks are the real-time path; this loop is the safety net, so a minute is
/// plenty without hammering GitLab.
const INTERVAL_SECS: u64 = 60;
/// Cap GitLab calls per cycle so a large stuck backlog can't stampede GitLab.
const MAX_PER_CYCLE: i64 = 100;
/// A non-terminal pipeline older than this is force-expired even if GitLab is
/// unreachable — no real pipeline stays runnable for days.
const FORCE_EXPIRE_DAYS: i64 = 2;

fn is_terminal(status: &str) -> bool {
    TERMINAL.contains(&status)
}

/// Background loop: heal the tracked-pipeline cache so a dropped webhook never
/// leaves a phantom non-terminal pipeline visible to `jeryu next` or the TUI.
pub async fn reconcile_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(INTERVAL_SECS));
    loop {
        interval.tick().await;
        match reconcile_tracked_pipelines(&state.db, &state.client, MAX_PER_CYCLE).await {
            Ok(healed) if healed > 0 => {
                info!(healed, "pipeline cache reconcile: healed stale tracked pipelines")
            }
            Ok(_) => {}
            Err(err) => debug!(error = %err, "pipeline cache reconcile failed (tolerated)"),
        }
    }
}

/// Reconcile non-terminal cached pipelines against live GitLab. Returns the
/// number of rows healed. Pure safety net: only advances a stale non-terminal
/// status to its true terminal value, or to `canceled` when the pipeline has
/// vanished (404) or is impossibly old.
pub async fn reconcile_tracked_pipelines(
    db: &Db,
    client: &GitlabClient,
    max: i64,
) -> Result<usize> {
    let stale = db.list_nonterminal_tracked_pipelines(max).await?;
    if stale.is_empty() {
        return Ok(0);
    }
    let force_cutoff = chrono::Utc::now() - chrono::Duration::days(FORCE_EXPIRE_DAYS);
    let mut healed = 0usize;

    for p in stale {
        let aged = chrono::DateTime::parse_from_rfc3339(&p.updated_at)
            .map(|t| t.with_timezone(&chrono::Utc) < force_cutoff)
            .unwrap_or(false);

        let resolved: Option<String> = match client.get_pipeline(p.project_id, p.pipeline_id).await
        {
            // GitLab says it's terminal but our cache disagrees: adopt the truth.
            Ok(live) if is_terminal(&live.status) && live.status != p.status => Some(live.status),
            // Still genuinely non-terminal in GitLab — leave it.
            Ok(_) => None,
            // 404 => pipeline gone; otherwise GitLab is unreachable. Either way,
            // force-expire impossibly old rows so they stop showing as active;
            // leave fresh rows for the next tick.
            Err(err) => {
                let gone = err.to_string().contains("404");
                if gone || aged {
                    Some("canceled".to_string())
                } else {
                    None
                }
            }
        };

        if let Some(status) = resolved {
            db.upsert_tracked_pipeline(&TrackedPipeline {
                pipeline_id: p.pipeline_id,
                project_id: p.project_id,
                ref_name: p.ref_name.clone(),
                sha: p.sha.clone(),
                status: status.clone(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .await?;
            healed += 1;
            debug!(
                pipeline_id = p.pipeline_id,
                old = %p.status,
                new = %status,
                "reconciled stale tracked pipeline"
            );
        }
    }
    Ok(healed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_statuses_are_classified() {
        for s in ["success", "failed", "canceled", "skipped"] {
            assert!(is_terminal(s), "{s} should be terminal");
        }
        for s in ["created", "pending", "running", "waiting_for_resource"] {
            assert!(!is_terminal(s), "{s} should be non-terminal");
        }
    }
}
