//! Owner: CI fast-track — webhook application (prune redundant MR pipeline jobs).
//! Proof: `cargo test -p jeryu --lib fast_track` (pure plan) + engine integration.
//! Invariants:
//!   - MR-only; never `main`/`master`/tags (those always run the full pipeline,
//!     which is the post-merge safety net for the aggression here).
//!   - Best-effort + tolerant: any GitLab error simply skips (no panic, no
//!     mutation). Idempotent — re-canceling an already-canceled job is harmless.

use tracing::info;

use crate::engine::EngineState;
use crate::fast_track::{JobView, plan_fast_track};
use crate::gitlab_client::Job;

/// The cheap-but-critical jobs that always re-run on the MR even when fast-tracked.
fn required_floor() -> Vec<String> {
    ["rust_fmt", "rust_clippy", "rust_build", "ci_runner_policy"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn to_views(jobs: &[Job]) -> Vec<JobView> {
    jobs.iter()
        .map(|j| JobView {
            id: j.id,
            name: j.name.clone(),
            status: j.status.clone(),
            allow_failure: j.allow_failure,
        })
        .collect()
}

/// On a re-pushed MR pipeline, cancel the jobs that already passed in the most
/// recent prior FAILED pipeline on this ref (keeping the failed jobs + the
/// required floor) so runners are not re-burned. The full pipeline still runs
/// post-merge on `main`.
pub(crate) async fn apply_fast_track(
    state: &EngineState,
    project_id: i64,
    ref_name: &str,
    new_pipeline_id: i64,
    new_sha: &str,
) {
    if project_id == 0
        || matches!(ref_name, "main" | "master")
        || ref_name.starts_with("refs/tags/")
    {
        return;
    }

    let pipelines = match state.client.list_pipelines(project_id, Some(ref_name)).await {
        Ok(p) => p,
        Err(_) => return,
    };
    // Most recent prior failed pipeline on this ref (a different commit).
    let Some(prior) = pipelines
        .iter()
        .find(|p| p.sha != new_sha && p.status == "failed")
    else {
        return;
    };

    let Ok(prior_jobs) = state.client.list_pipeline_jobs(project_id, prior.id).await else {
        return;
    };
    let Ok(new_jobs) = state
        .client
        .list_pipeline_jobs(project_id, new_pipeline_id)
        .await
    else {
        return;
    };
    if new_jobs.is_empty() {
        // Jobs not created yet on this pipeline — a later pending/running event retries.
        return;
    }

    let plan = plan_fast_track(&to_views(&prior_jobs), &to_views(&new_jobs), &required_floor());
    if !plan.eligible {
        return;
    }

    let mut canceled = 0usize;
    for jid in &plan.cancel_job_ids {
        if state.client.cancel_job(project_id, *jid).await.is_ok() {
            canceled += 1;
        }
    }
    if canceled == 0 {
        return;
    }

    let _ = state
        .db
        .append_event(
            "fast_track_decided",
            Some(project_id),
            None,
            "engine",
            &serde_json::json!({
                "new_pipeline_id": new_pipeline_id,
                "prior_pipeline_id": prior.id,
                "ref": ref_name,
                "must_run": plan.must_run,
                "canceled_jobs": canceled,
                "reason": plan.reason,
            })
            .to_string(),
        )
        .await;
    info!(
        project_id,
        new_pipeline_id,
        prior_pipeline_id = prior.id,
        canceled,
        "fast-track: pruned previously-passed jobs on re-pushed MR pipeline"
    );
}
