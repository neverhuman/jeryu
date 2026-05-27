//! Owner: Interactive TUI subsystem — Delivery promotion pipeline
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure node/edge construction + status derivation from
//! `DeploymentProgress` and optional release attempt view; no I/O.

use super::DeploymentProgress;
use crate::release::ReleaseAttemptView;
use crate::tui::workflow::model::{
    CanonicalPhase, Environment, WorkflowEdge, WorkflowEdgeKind, WorkflowNode, WorkflowNodeKind,
    WorkflowStatus,
};

pub(super) fn promote_node(
    nodes: &mut Vec<WorkflowNode>,
    edges: &mut Vec<WorkflowEdge>,
    pr_number: u64,
    env: Environment,
    status: WorkflowStatus,
    dep_id: &str,
    canary_url: Option<String>,
) -> String {
    let phase = match env {
        Environment::Local => CanonicalPhase::PromoteLocal,
        Environment::Dev => CanonicalPhase::PromoteDev,
        Environment::Prod => CanonicalPhase::PromoteProd,
    };
    let id = format!("pr{}::promote-{}", pr_number, env.label());
    nodes.push(WorkflowNode {
        id: id.clone(),
        label: format!("promote → {}", env.label()),
        command: Some(format!("jeryu release promote --env {}", env.label())),
        kind: WorkflowNodeKind::Promote { env },
        status,
        required: matches!(env, Environment::Dev | Environment::Prod),
        deps: vec![dep_id.to_string()],
        reason: canary_url.map(|u| format!("Canary URL: {}", u)),
        tags: vec![phase.slug().into()],
        ..Default::default()
    });
    edges.push(WorkflowEdge {
        from: dep_id.to_string(),
        to: id.clone(),
        kind: WorkflowEdgeKind::Dependency,
    });
    id
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
    // Default::default() is the documented empty semantic here: an unset
    // `phase` falls into the catch-all Waiting arm below.
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
