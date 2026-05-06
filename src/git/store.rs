//! Owner: Git event persistence
//! Proof: `cargo test -p jeryu -- git_store`
//! Invariants: All SQL remains centralized in `state::Db` methods.

use anyhow::Result;
use chrono::Utc;

use crate::git::invocation::GitInvocation;
use crate::git::mirror::PushMirrorPlan;
use crate::git::event::GitCommandEvent;
use crate::git::snapshot::GitSnapshot;
use crate::state::Db;
use crate::state::GitMirrorJob;

pub async fn store_git_event(db: &Db, event: &GitCommandEvent) -> Result<i64> {
    db.record_git_command_event(event).await
}

pub async fn store_git_side_effects(
    db: &Db,
    invocation: &GitInvocation,
    before: &GitSnapshot,
    after: Option<&GitSnapshot>,
    mirror_status: &str,
    mirror_plan: Option<&PushMirrorPlan>,
    sidecar_status: &mut String,
) {
    if let Some(plan) = mirror_plan {
        let branch_name = match plan.ref_name.as_ref() {
            Some(name) => name.clone(),
            None => "HEAD".to_string(),
        };
        let job = GitMirrorJob {
            id: 0,
            request_id: invocation.request_id.clone(),
            remote_name: plan.remote_name.clone(),
            branch_name: Some(branch_name),
            status: mirror_status.to_string(),
            detail: plan.git_args.join(" "),
            created_at: Utc::now().to_rfc3339(),
        };
        if let Err(err) = db.record_git_mirror_job(&job).await {
            tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git mirror job");
            *sidecar_status = "db_write_failed".to_string();
        }
    }

    if let Some(after) = after {
        let changed = before.head != after.head || before.branch != after.branch;
        if changed || invocation.is_push() {
            let ref_name = match after.branch.clone() {
                Some(name) => name,
                None => match before.branch.clone() {
                    Some(name) => name,
                    None => "HEAD".to_string(),
                },
            };
            let status = if invocation.is_push() {
                mirror_status.to_string()
            } else {
                "observed".to_string()
            };
            if let Err(err) = db.record_git_ref_change(
                invocation.request_id.clone(),
                ref_name,
                before.head.clone(),
                after.head.clone(),
                status,
                Utc::now().to_rfc3339(),
            ).await {
                tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git ref change");
                *sidecar_status = "db_write_failed".to_string();
            }
        }
    }
}
