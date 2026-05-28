//! Owner: Git passthrough execution and event recording
//! Proof: `cargo test -p jeryu -- git_passthrough`
//! Invariants: Each command invokes the real git binary exactly once before any optional mirror step.

use anyhow::Result;

use crate::git::event::GitCommandEvent;
use crate::git::invocation::GitInvocation;
use crate::git::mirror::{mirror_push_plan, parse_push_mirror_plan};
use crate::git::policy::{mirror_remote, should_mirror, strict_mode_enabled};
use crate::git::snapshot::{capture, snapshot_or_empty};
use crate::git::store::{store_git_event, store_git_side_effects};
use crate::git::system::SystemGit;
use crate::state::Db;

pub async fn execute_git(db: Option<&Db>, argv: &[String]) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    if crate::access::repo_origin_is_local_http(&cwd).unwrap_or(false) {
        eprintln!(
            "jeryu git: local GitLab HTTP origins are forbidden; run `jeryu access repair --repo . --yes`"
        );
        return Ok(1);
    }
    let invocation = GitInvocation::new(&cwd, argv.to_vec());
    let git = SystemGit::resolve()?;
    let before = snapshot_or_empty(&cwd);
    let git_args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let status = git.status(&cwd, &git_args)?;
    let exit_code = status.code().unwrap_or(1);
    let after = capture(&cwd).ok();

    let mut mirror_status = "not_attempted".to_string();
    let mut sidecar_status = "ok".to_string();
    let mut final_exit_code = exit_code;
    if exit_code == 0 && should_mirror(invocation.class, &invocation.argv) {
        let remote = mirror_remote();
        let mirror_plan = parse_push_mirror_plan(argv, &remote);
        let mirrored = mirror_plan
            .as_ref()
            .map(|plan| mirror_push_plan(&cwd, plan).unwrap_or(false))
            .unwrap_or(false);
        mirror_status = if mirrored {
            "mirrored".to_string()
        } else {
            "mirror_failed".to_string()
        };
        if !mirrored && strict_mode_enabled() {
            final_exit_code = 1;
        }
        if let Some(db) = db {
            record_git_side_effects(
                db,
                &invocation,
                &before,
                after.as_ref(),
                &mirror_status,
                mirror_plan.as_ref(),
                &mut sidecar_status,
            )
            .await;
        }
    }

    if let Some(db) = db {
        record_git_command_event(
            db,
            &invocation,
            before,
            after,
            final_exit_code,
            sidecar_status,
            mirror_status,
        )
        .await;
    }

    Ok(final_exit_code)
}

pub async fn execute_git_once(argv: &[String]) -> Result<i32> {
    execute_git(None, argv).await
}

async fn record_git_side_effects(
    db: &Db,
    invocation: &GitInvocation,
    before: &crate::git::snapshot::GitSnapshot,
    after: Option<&crate::git::snapshot::GitSnapshot>,
    mirror_status: &str,
    mirror_plan: Option<&crate::git::mirror::PushMirrorPlan>,
    sidecar_status: &mut String,
) {
    store_git_side_effects(
        db,
        invocation,
        before,
        after,
        mirror_status,
        mirror_plan,
        sidecar_status,
    )
    .await;
}

async fn record_git_command_event(
    db: &Db,
    invocation: &GitInvocation,
    before: crate::git::snapshot::GitSnapshot,
    after: Option<crate::git::snapshot::GitSnapshot>,
    final_exit_code: i32,
    sidecar_status: String,
    mirror_status: String,
) {
    let event = GitCommandEvent::from_invocation(
        invocation,
        before,
        after,
        final_exit_code,
        sidecar_status,
        mirror_status,
    );
    if let Err(err) = store_git_event(db, &event).await {
        tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git command event");
    }
}
