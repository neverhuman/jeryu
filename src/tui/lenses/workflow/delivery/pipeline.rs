//! Canonical PR pipeline assembly.

use chrono::{DateTime, Utc};

use crate::{
    release::ReleaseAttemptView,
    tui::{
        lenses::workflow::{
            delivery::{
                agent_review::{agent_review_reason, agent_review_receipt_status, demo_agent_call},
                auto_merge::auto_merge_gate_status,
                inputs::PrInput,
                post_merge::{
                    aggregate_status, derive_furthest_phase, derive_pr_status, pick_current_node,
                    relabel_phases_to_canonical,
                },
                promotion::{deployment_canary_url, deployment_status, promote_node},
            },
            model::*,
        },
        workflow::builder,
    },
};

pub(super) fn build_pr_view(
    pr: &PrInput,
    release: Option<&ReleaseAttemptView>,
    now: DateTime<Utc>,
) -> PullRequestView {
    let snapshot = build_canonical_pipeline(pr, release);
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
        repo_alias: pr.repo_alias.clone(),
        repo_slug: pr.repo_slug.clone(),
    }
}

fn build_canonical_pipeline(
    pr: &PrInput,
    release: Option<&ReleaseAttemptView>,
) -> WorkflowSnapshot {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

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
    let pre_ci = aggregate_status(&pr.pre_merge_tests);

    let agent_pre_id = format!("pr{}::agent-review-pre", pr.number);
    let agent_pre_status = agent_review_receipt_status(pre_ci, &pr.labels);
    nodes.push(WorkflowNode {
        id: agent_pre_id.clone(),
        label: "agent code review".into(),
        command: Some("autonomy mr validate --emit-status".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        },
        status: agent_pre_status,
        required: true,
        deps: pre_test_ids.clone(),
        reason: Some(agent_review_reason(agent_pre_status)),
        tags: vec![CanonicalPhase::AgentReviewPreMerge.slug().into()],
        agent_call: demo_agent_call(agent_pre_status, AgentStage::PreMerge),
        ..Default::default()
    });
    for dep in &pre_test_ids {
        edges.push(WorkflowEdge {
            from: dep.clone(),
            to: agent_pre_id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
    }

    let auto_merge_id = format!("pr{}::auto-merge", pr.number);
    let auto_merge_status = auto_merge_gate_status(pre_ci, agent_pre_status);
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

    let post_test_ids = add_post_merge_tests(pr, &auto_merge_id, &mut nodes, &mut edges);
    let post_ci = aggregate_status(&pr.post_merge_tests);

    let agent_post_id = format!("pr{}::agent-review-post", pr.number);
    let agent_post_status = if pr.merged_into_main {
        agent_review_receipt_status(post_ci, &pr.labels)
    } else {
        WorkflowStatus::Waiting
    };
    nodes.push(WorkflowNode {
        id: agent_post_id.clone(),
        label: "agent regression review".into(),
        command: Some("autonomy mr validate --post-merge --emit-status".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PostMerge,
        },
        status: agent_post_status,
        required: false,
        deps: post_test_ids.clone(),
        reason: Some(agent_review_reason(agent_post_status)),
        tags: vec![CanonicalPhase::AgentReviewPostMerge.slug().into()],
        agent_call: demo_agent_call(agent_post_status, AgentStage::PostMerge),
        ..Default::default()
    });
    for dep in &post_test_ids {
        edges.push(WorkflowEdge {
            from: dep.clone(),
            to: agent_post_id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
    }

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
        from: agent_post_id,
        to: build_id.clone(),
        kind: WorkflowEdgeKind::Dependency,
    });

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

    let monitor_id = format!("pr{}::monitor", pr.number);
    nodes.push(WorkflowNode {
        id: monitor_id.clone(),
        label: "monitor production".into(),
        kind: WorkflowNodeKind::Monitor,
        status: pr.deployment.monitor_status,
        required: false,
        deps: vec![prod_id.clone()],
        reason: deployment_canary_url(&pr.deployment, release).map(|url| format!("Canary: {url}")),
        tags: vec![CanonicalPhase::MonitorRollback.slug().into()],
        ..Default::default()
    });
    edges.push(WorkflowEdge {
        from: prod_id,
        to: monitor_id,
        kind: WorkflowEdgeKind::Dependency,
    });

    let title = format!("PR #{} — {}", pr.number, pr.title);
    let mut snapshot = builder::build_snapshot(
        nodes,
        edges,
        &title,
        "delivery",
        0.0,
        WorkflowSource::LivePipeline,
    );
    relabel_phases_to_canonical(&mut snapshot);
    snapshot
}

fn add_post_merge_tests(
    pr: &PrInput,
    auto_merge_id: &str,
    nodes: &mut Vec<WorkflowNode>,
    edges: &mut Vec<WorkflowEdge>,
) -> Vec<String> {
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
                deps: vec![auto_merge_id.into()],
                tags: vec![CanonicalPhase::PostMergeCI.slug().into()],
                ..Default::default()
            });
            edges.push(WorkflowEdge {
                from: auto_merge_id.into(),
                to: id.clone(),
                kind: WorkflowEdgeKind::Dependency,
            });
            post_test_ids.push(id);
        }
    } else {
        let id = format!("pr{}::post::pending", pr.number);
        nodes.push(WorkflowNode {
            id: id.clone(),
            label: "post-merge tests".into(),
            kind: WorkflowNodeKind::IntegrationTest,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: vec![auto_merge_id.into()],
            reason: Some("Awaiting auto-merge.".into()),
            tags: vec![CanonicalPhase::PostMergeCI.slug().into()],
            ..Default::default()
        });
        edges.push(WorkflowEdge {
            from: auto_merge_id.into(),
            to: id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
        post_test_ids.push(id);
    }
    post_test_ids
}
