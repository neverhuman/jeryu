//! Owner: Interactive TUI subsystem — canonical pipeline construction
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure construction; never mutates inputs.

use chrono::{DateTime, Utc};

use super::builder;
use super::model::*;
use crate::release::ReleaseAttemptView;

use super::delivery::PrInput;
use super::status::{
    agent_review_auto_pass_status, aggregate_status, deployment_canary_url, deployment_status,
    derive_furthest_phase, derive_pr_status, pick_current_node, stub_auto_merge_status,
};

pub(super) fn build_pr_view(
    pr: &PrInput,
    release: Option<&ReleaseAttemptView>,
    now: DateTime<Utc>,
) -> PullRequestView {
    let snapshot = build_canonical_pipeline(pr, release, now);
    let phase = derive_furthest_phase(&snapshot);
    let status = derive_pr_status(pr, &snapshot);
    let current_node_id = pick_current_node(&snapshot);
    let age_secs = (now - pr.created_at).num_seconds().max(0) as u64;

    PullRequestView {
        number: pr.number,
        title: pr.title.clone(),
        author: pr.author.clone(),
        head_sha: pr.head_sha.clone(),
        status,
        phase,
        mergeable: phase >= CanonicalPhase::AutoMerge && status != PrStatus::Blocked,
        ci_summary: snapshot.summary.clone(),
        age_secs,
        draft: pr.draft,
        labels: pr.labels.clone(),
        current_node_id,
        snapshot,
        // Risk classification not yet wired from autonomy scorecard; default
        // to R2 (moderate) so the cockpit always has a value to render.
        risk: "R2".into(),
    }
}

/// Build the canonical-pipeline DAG for a single PR.
fn build_canonical_pipeline(
    pr: &PrInput,
    release: Option<&ReleaseAttemptView>,
    now: DateTime<Utc>,
) -> WorkflowSnapshot {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // ── Phase: Pre-merge CI (one node per test) ─────────────────────
    let mut pre_test_ids = Vec::new();
    for test in &pr.pre_merge_tests {
        let id = format!("pr{}::pre::{}", pr.number, test.id);
        nodes.push(WorkflowNode {
            id: id.clone(),
            label: test.label.clone(),
            command: Some(test.command.clone()),
            kind: WorkflowNodeKind::UnitTest,
            status: test.status,
            required: true,
            critical_path: test.critical_path,
            progress_pct: test.progress_pct,
            eta_secs: test.eta_secs,
            duration_secs: test.duration_secs,
            reason: test.reason.clone(),
            tags: vec![CanonicalPhase::PreMergeCI.slug().into()],
            ..Default::default()
        });
        pre_test_ids.push(id);
    }
    let pre_ci_aggregate = aggregate_status(&pr.pre_merge_tests);

    // ── Phase: Agent review (pre-merge) ──────────────────────────────────────────
    let agent_pre_id = format!("pr{}::agent-review-pre", pr.number);
    let agent_pre_status = agent_review_auto_pass_status(pre_ci_aggregate, pr.created_at, now);
    nodes.push(WorkflowNode {
        id: agent_pre_id.clone(),
        label: "agent code review".into(),
        command: Some("(auto-pass) jeryu agent review --pre-merge".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        },
        status: agent_pre_status,
        required: true,
        deps: pre_test_ids.clone(),
        reason: Some("Auto-passes once pre-merge CI is green (agent review pending).".into()),
        tags: vec![CanonicalPhase::AgentReviewPreMerge.slug().into()],
        ..Default::default()
    });
    for dep in &pre_test_ids {
        edges.push(WorkflowEdge {
            from: dep.clone(),
            to: agent_pre_id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
    }

    // ── Phase: Auto-merge ───────────────────────────────────────────
    let auto_merge_id = format!("pr{}::auto-merge", pr.number);
    let auto_merge_status = stub_auto_merge_status(pre_ci_aggregate, agent_pre_status);
    nodes.push(WorkflowNode {
        id: auto_merge_id.clone(),
        label: "auto-merge to main".into(),
        command: Some("(policy) jeryu git auto-merge".into()),
        kind: WorkflowNodeKind::AutoMerge,
        status: auto_merge_status,
        required: true,
        deps: vec![agent_pre_id.clone()],
        reason: Some("Policy: PR auto-merges when pre-merge CI passes.".into()),
        tags: vec![CanonicalPhase::AutoMerge.slug().into()],
        ..Default::default()
    });
    edges.push(WorkflowEdge {
        from: agent_pre_id.clone(),
        to: auto_merge_id.clone(),
        kind: WorkflowEdgeKind::Dependency,
    });

    // ── Phase: Post-merge CI (only after auto-merge passes) ────────
    let mut post_test_ids = Vec::new();
    if pr.merged_into_main {
        for test in &pr.post_merge_tests {
            let id = format!("pr{}::post::{}", pr.number, test.id);
            nodes.push(WorkflowNode {
                id: id.clone(),
                label: test.label.clone(),
                command: Some(test.command.clone()),
                kind: WorkflowNodeKind::IntegrationTest,
                status: test.status,
                required: true,
                critical_path: test.critical_path,
                progress_pct: test.progress_pct,
                eta_secs: test.eta_secs,
                duration_secs: test.duration_secs,
                deps: vec![auto_merge_id.clone()],
                tags: vec![CanonicalPhase::PostMergeCI.slug().into()],
                ..Default::default()
            });
            edges.push(WorkflowEdge {
                from: auto_merge_id.clone(),
                to: id.clone(),
                kind: WorkflowEdgeKind::Dependency,
            });
            post_test_ids.push(id);
        }
    } else {
        // Waiting node for unmerged PRs so the post-merge phase rail entry isn't empty.
        let id = format!("pr{}::post::pending", pr.number);
        nodes.push(WorkflowNode {
            id: id.clone(),
            label: "post-merge tests".into(),
            kind: WorkflowNodeKind::IntegrationTest,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: vec![auto_merge_id.clone()],
            reason: Some("Awaiting auto-merge.".into()),
            tags: vec![CanonicalPhase::PostMergeCI.slug().into()],
            ..Default::default()
        });
        edges.push(WorkflowEdge {
            from: auto_merge_id.clone(),
            to: id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
        post_test_ids.push(id);
    }
    let post_ci_aggregate = aggregate_status(&pr.post_merge_tests);

    // ── Phase: Agent review (post-merge) ──────────────────────────────────────────
    let agent_post_id = format!("pr{}::agent-review-post", pr.number);
    let agent_post_status = if pr.merged_into_main {
        agent_review_auto_pass_status(post_ci_aggregate, pr.created_at, now)
    } else {
        WorkflowStatus::Waiting
    };
    nodes.push(WorkflowNode {
        id: agent_post_id.clone(),
        label: "agent regression review".into(),
        command: Some("(auto-pass) jeryu agent review --post-merge".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PostMerge,
        },
        status: agent_post_status,
        required: false,
        deps: post_test_ids.clone(),
        reason: Some("Auto-passes once post-merge CI is green (agent review pending).".into()),
        tags: vec![CanonicalPhase::AgentReviewPostMerge.slug().into()],
        ..Default::default()
    });
    for dep in &post_test_ids {
        edges.push(WorkflowEdge {
            from: dep.clone(),
            to: agent_post_id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
    }

    // ── Phase: Build immutable artifact ────────────────────────────
    let build_id = format!("pr{}::build-artifact", pr.number);
    nodes.push(WorkflowNode {
        id: build_id.clone(),
        label: "build immutable image".into(),
        command: Some("nix build .#jeryu --out-link result".into()),
        kind: WorkflowNodeKind::BuildArtifact,
        status: pr.deployment.build_status,
        required: true,
        deps: vec![agent_post_id.clone()],
        progress_pct: pr.deployment.build_progress,
        tags: vec![CanonicalPhase::BuildArtifact.slug().into()],
        ..Default::default()
    });
    edges.push(WorkflowEdge {
        from: agent_post_id.clone(),
        to: build_id.clone(),
        kind: WorkflowEdgeKind::Dependency,
    });

    // ── Phase: Promote local → dev → prod ──────────────────────────
    let local_id = promote_node(
        &mut nodes,
        &mut edges,
        pr.number,
        Environment::Local,
        pr.deployment.local_status,
        &build_id,
        None,
    );
    let dev_id = promote_node(
        &mut nodes,
        &mut edges,
        pr.number,
        Environment::Dev,
        deployment_status(&pr.deployment, Environment::Dev, release),
        &local_id,
        deployment_canary_url(&pr.deployment, release),
    );
    let prod_id = promote_node(
        &mut nodes,
        &mut edges,
        pr.number,
        Environment::Prod,
        deployment_status(&pr.deployment, Environment::Prod, release),
        &dev_id,
        None,
    );

    // ── Phase: Monitor + rollback ──────────────────────────────────
    let monitor_id = format!("pr{}::monitor", pr.number);
    nodes.push(WorkflowNode {
        id: monitor_id.clone(),
        label: "monitor production".into(),
        kind: WorkflowNodeKind::Monitor,
        status: pr.deployment.monitor_status,
        required: false,
        deps: vec![prod_id.clone()],
        reason: deployment_canary_url(&pr.deployment, release).map(|u| format!("Canary: {}", u)),
        tags: vec![CanonicalPhase::MonitorRollback.slug().into()],
        ..Default::default()
    });
    edges.push(WorkflowEdge {
        from: prod_id,
        to: monitor_id,
        kind: WorkflowEdgeKind::Dependency,
    });

    let title = format!("PR #{} — {}", pr.number, pr.title);
    let mut snap = builder::build_snapshot(
        nodes,
        edges,
        &title,
        "delivery",
        0.0,
        WorkflowSource::LivePipeline,
    );
    // Phase titles default to "Phase N — ..."; replace with canonical labels
    // by depth (best-effort: phases are produced in depth order).
    relabel_phases_to_canonical(&mut snap);
    snap
}

fn promote_node(
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

fn relabel_phases_to_canonical(snap: &mut WorkflowSnapshot) {
    for phase in snap.phases.iter_mut() {
        // Inspect the first node in the phase to determine its canonical slug.
        if let Some(first_id) = phase.node_ids.first()
            && let Some(node) = snap.nodes.iter().find(|n| &n.id == first_id)
            && let Some(slug) = node.tags.first()
            && let Some(cp) = CanonicalPhase::ALL.iter().find(|p| p.slug() == slug)
        {
            phase.title = cp.title().to_string();
            phase.id = cp.slug().to_string();
        }
    }
}
