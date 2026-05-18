//! Owner: Interactive TUI subsystem — status aggregation and phase/PR derivation
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure functions; no side effects.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use super::model::*;
use crate::release::ReleaseAttemptView;

use super::delivery::{AGENT_REVIEW_DELAY_SECS, DeploymentProgress, PrInput, TestSpec};

/// Aggregate child-node statuses into a parent gate's status.
pub(super) fn aggregate_status(tests: &[TestSpec]) -> WorkflowStatus {
    if tests.is_empty() {
        return WorkflowStatus::Waiting;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Error) {
        return WorkflowStatus::Error;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Running) {
        return WorkflowStatus::Running;
    }
    if tests.iter().any(|t| t.status == WorkflowStatus::Blocked) {
        return WorkflowStatus::Blocked;
    }
    if tests.iter().all(|t| t.status.is_terminal()) {
        WorkflowStatus::Ran
    } else {
        WorkflowStatus::Waiting
    }
}

pub(super) fn agent_review_auto_pass_status(
    upstream: WorkflowStatus,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> WorkflowStatus {
    match upstream {
        WorkflowStatus::Ran | WorkflowStatus::Cached => {
            if now - created_at >= ChronoDuration::seconds(AGENT_REVIEW_DELAY_SECS) {
                WorkflowStatus::Ran
            } else {
                WorkflowStatus::Running
            }
        }
        WorkflowStatus::Error => WorkflowStatus::Blocked,
        WorkflowStatus::Running | WorkflowStatus::Waiting => WorkflowStatus::Waiting,
        WorkflowStatus::Skipped => WorkflowStatus::Skipped,
        WorkflowStatus::Blocked => WorkflowStatus::Blocked,
        WorkflowStatus::Unknown => WorkflowStatus::Unknown,
    }
}

pub(super) fn stub_auto_merge_status(
    pre_ci: WorkflowStatus,
    agent_pre: WorkflowStatus,
) -> WorkflowStatus {
    match (pre_ci, agent_pre) {
        (WorkflowStatus::Ran | WorkflowStatus::Cached, WorkflowStatus::Ran) => WorkflowStatus::Ran,
        (WorkflowStatus::Error, _) | (_, WorkflowStatus::Error) | (_, WorkflowStatus::Blocked) => {
            WorkflowStatus::Blocked
        }
        _ => WorkflowStatus::Waiting,
    }
}

pub(super) fn deployment_status(
    dep: &DeploymentProgress,
    env: Environment,
    release: Option<&ReleaseAttemptView>,
) -> WorkflowStatus {
    let from_dep = match env {
        Environment::Local => dep.local_status,
        Environment::Dev => dep.dev_status,
        Environment::Prod => dep.prod_status,
    };
    if from_dep != WorkflowStatus::Waiting {
        return from_dep;
    }
    // Hydrate from release state when no explicit status set.
    if let Some(view) = release {
        return status_from_release_phase(env, view);
    }
    WorkflowStatus::Waiting
}

fn status_from_release_phase(env: Environment, view: &ReleaseAttemptView) -> WorkflowStatus {
    let phase = view.phase.as_deref().unwrap_or("");
    match (env, phase) {
        (Environment::Dev, "canary") | (Environment::Dev, "canary_e2e") => WorkflowStatus::Running,
        (Environment::Dev, "promoted") => WorkflowStatus::Ran,
        (Environment::Prod, "promoted") => WorkflowStatus::Running,
        _ => WorkflowStatus::Waiting,
    }
}

pub(super) fn deployment_canary_url(
    dep: &DeploymentProgress,
    release: Option<&ReleaseAttemptView>,
) -> Option<String> {
    match dep.canary_url.clone() {
        Some(url) => Some(url),
        None => release.and_then(|v| v.canary_public_url.clone()),
    }
}

pub(super) fn derive_furthest_phase(snap: &WorkflowSnapshot) -> CanonicalPhase {
    // Walk canonical phases in order and stop at the first that isn't all-terminal.
    let mut furthest = CanonicalPhase::PreMergeCI;
    for phase in CanonicalPhase::ALL {
        let nodes: Vec<_> = snap
            .nodes
            .iter()
            .filter(|n| n.tags.iter().any(|t| t == phase.slug()))
            .collect();
        if nodes.is_empty() {
            continue;
        }
        let any_active = nodes
            .iter()
            .any(|n| matches!(n.status, WorkflowStatus::Running));
        let any_blocked = nodes
            .iter()
            .any(|n| matches!(n.status, WorkflowStatus::Blocked | WorkflowStatus::Error));
        let all_terminal = nodes.iter().all(|n| n.status.is_terminal());
        if any_active || any_blocked {
            return phase;
        }
        if all_terminal {
            furthest = phase;
        }
    }
    furthest
}

pub(super) fn derive_pr_status(pr: &PrInput, snap: &WorkflowSnapshot) -> PrStatus {
    if pr.draft {
        return PrStatus::Draft;
    }
    if snap
        .nodes
        .iter()
        .any(|n| matches!(n.status, WorkflowStatus::Error))
    {
        return PrStatus::Blocked;
    }
    if snap
        .nodes
        .iter()
        .any(|n| matches!(n.status, WorkflowStatus::Blocked))
    {
        return PrStatus::Blocked;
    }
    if pr.merged_into_main {
        return PrStatus::Merged;
    }
    if snap
        .nodes
        .iter()
        .any(|n| matches!(n.status, WorkflowStatus::Running))
    {
        return PrStatus::Running;
    }
    PrStatus::Open
}

pub(super) fn pick_current_node(snap: &WorkflowSnapshot) -> Option<String> {
    // Preference: first error → first running → first waiting → none.
    if let Some(n) = snap
        .nodes
        .iter()
        .find(|n| matches!(n.status, WorkflowStatus::Error | WorkflowStatus::Blocked))
    {
        return Some(n.id.clone());
    }
    if let Some(n) = snap
        .nodes
        .iter()
        .find(|n| matches!(n.status, WorkflowStatus::Running))
    {
        return Some(n.id.clone());
    }
    snap.nodes
        .iter()
        .find(|n| matches!(n.status, WorkflowStatus::Waiting))
        .map(|n| n.id.clone())
}
