//! Owner: Interactive TUI subsystem — Delivery view collector
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure construction; never mutates inputs.
//!
//! Assembles a `DeliverySnapshot` (multiple PRs, each with a canonical-pipeline
//! `WorkflowSnapshot`) from whatever inputs are available.
//!
//! Two controller nodes:
//!   * `AgentReview { stage }` — driven by receipt status emitted by reviewers.
//!   * `AutoMerge` — passes once every pre-merge node has succeeded
//!     (mirrors the user-stated policy: PRs auto-merge when pre-merge CI
//!     passes).

use chrono::{DateTime, Utc};

use super::builder;
use super::model::*;
use crate::release::ReleaseAttemptView;

mod agent_review;
mod auto_merge;
mod ci;
mod post_merge;
mod promotion;

pub use post_merge::build_demo_delivery;

use agent_review::{agent_review_reason, agent_review_receipt_status, demo_agent_call};
use auto_merge::auto_merge_gate_status;
use ci::aggregate_status;
use promotion::{deployment_canary_url, deployment_status, promote_node};

/// Kept as a public API compatibility constant. Receipt-backed agent review
/// no longer auto-passes after this delay.
pub const AGENT_REVIEW_AUTO_PASS_DELAY_SECS: i64 = 5;

/// Lightweight input describing a single PR to render.
#[derive(Debug, Clone)]
pub struct PrInput {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
    pub draft: bool,
    pub labels: Vec<String>,
    /// Per-PR test nodes for the pre-merge CI phase. Each is a real test
    /// the CI will execute; their statuses drive the pre-merge phase rollup.
    pub pre_merge_tests: Vec<TestSpec>,
    /// True once the PR has cleared pre-merge CI + agent review + auto-merge
    /// and has been merged into main.
    pub merged_into_main: bool,
    /// Post-merge test nodes (only relevant once `merged_into_main`).
    pub post_merge_tests: Vec<TestSpec>,
    /// Build/promotion progress for merged PRs; ignored for unmerged PRs.
    pub deployment: DeploymentProgress,
    /// Fleet alias of the repo this PR lives in. Populated by repo-aware
    /// collectors; `None` for legacy single-repo callers.
    pub repo_alias: Option<String>,
    /// Fleet slug of the repo (`"owner/repo"`).
    pub repo_slug: Option<String>,
}

/// A single test/check that runs as part of a CI batch.
#[derive(Debug, Clone)]
pub struct TestSpec {
    pub id: String,
    pub label: String,
    pub command: String,
    pub status: WorkflowStatus,
    pub progress_pct: Option<u16>,
    pub eta_secs: Option<u64>,
    pub duration_secs: Option<f64>,
    pub reason: Option<String>,
    pub critical_path: bool,
}

/// Tracks how far through artifact-build + environment promotion a merged
/// PR has progressed.
#[derive(Debug, Clone, Default)]
pub struct DeploymentProgress {
    pub build_status: WorkflowStatus,
    pub build_progress: Option<u16>,
    pub local_status: WorkflowStatus,
    pub dev_status: WorkflowStatus,
    pub prod_status: WorkflowStatus,
    pub monitor_status: WorkflowStatus,
    pub canary_url: Option<String>,
}

/// Build a `DeliverySnapshot` from a list of PR inputs and optional release
/// state. PRs are rendered in the order supplied; selected_pr_idx defaults
/// to 0 unless restored by the caller.
pub fn collect_delivery_snapshot(
    prs: &[PrInput],
    release: Option<&ReleaseAttemptView>,
) -> DeliverySnapshot {
    let now = Utc::now();
    let pull_requests: Vec<PullRequestView> = prs
        .iter()
        .map(|pr| build_pr_view(pr, release, now))
        .collect();

    let fleet_summary = compute_fleet_summary(&pull_requests, release);

    DeliverySnapshot {
        generated_at: now,
        pull_requests,
        selected_pr_idx: 0,
        fleet_summary,
        outdated: false,
        kill_bell_state: "armed".into(),
    }
}

fn build_pr_view(
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
        repo_alias: pr.repo_alias.clone(),
        repo_slug: pr.repo_slug.clone(),
    }
}

/// Build the canonical-pipeline DAG for a single PR.
fn build_canonical_pipeline(
    pr: &PrInput,
    release: Option<&ReleaseAttemptView>,
    _now: DateTime<Utc>,
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

    // ── Phase: Agent review (pre-merge) — receipt-backed ───────────
    let agent_pre_id = format!("pr{}::agent-review-pre", pr.number);
    let agent_pre_status = agent_review_receipt_status(pre_ci_aggregate, &pr.labels);
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

    // ── Phase: Auto-merge ───────────────────────────────────────────
    let auto_merge_id = format!("pr{}::auto-merge", pr.number);
    let auto_merge_status = auto_merge_gate_status(pre_ci_aggregate, agent_pre_status);
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
        // Pending Waiting node so the post-merge phase rail entry isn't empty.
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

    // ── Phase: Agent review (post-merge) — receipt-backed ──────────
    let agent_post_id = format!("pr{}::agent-review-post", pr.number);
    let agent_post_status = if pr.merged_into_main {
        agent_review_receipt_status(post_ci_aggregate, &pr.labels)
    } else {
        WorkflowStatus::Waiting
    };
    nodes.push(WorkflowNode {
        id: agent_post_id.clone(),
        label: "agent regression review".into(),
        command: Some("autonomy mr validate --emit-status".into()),
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

fn derive_furthest_phase(snap: &WorkflowSnapshot) -> CanonicalPhase {
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

fn derive_pr_status(pr: &PrInput, snap: &WorkflowSnapshot) -> PrStatus {
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

fn pick_current_node(snap: &WorkflowSnapshot) -> Option<String> {
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

fn compute_fleet_summary(
    prs: &[PullRequestView],
    release: Option<&ReleaseAttemptView>,
) -> FleetSummary {
    let open_prs = prs
        .iter()
        .filter(|pr| pr.status != PrStatus::Closed)
        .count() as u32;
    let ready_to_ship = prs
        .iter()
        .filter(|pr| pr.phase >= CanonicalPhase::PromoteProd)
        .count() as u32;
    let running = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Running)
        .count() as u32;
    let blocked = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Blocked)
        .count() as u32;
    let merged_today = prs
        .iter()
        .filter(|pr| pr.status == PrStatus::Merged)
        .count() as u32;

    let canary_in_flight = prs.iter().any(|pr| pr.phase == CanonicalPhase::PromoteDev);
    let prod_in_flight = prs.iter().any(|pr| pr.phase == CanonicalPhase::PromoteProd);

    let release_canary_url = release.and_then(|v| v.canary_public_url.clone());
    let node_canary_url = || {
        prs.iter().find_map(|pr| {
            pr.snapshot.nodes.iter().find_map(|n| {
                matches!(
                    n.kind,
                    WorkflowNodeKind::Promote {
                        env: Environment::Dev
                    }
                )
                .then(|| n.reason.clone())
                .flatten()
            })
        })
    };
    let canary_url = match release_canary_url {
        Some(url) => Some(url),
        None => node_canary_url(),
    };

    FleetSummary {
        open_prs,
        ready_to_ship,
        running,
        blocked,
        merged_today,
        canary_in_flight,
        prod_in_flight,
        canary_url,
        top_blocker: None,
    }
}

// ─── PartialOrd for CanonicalPhase ───────────────────────────────────────
impl PartialOrd for CanonicalPhase {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CanonicalPhase {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // CanonicalPhase::ALL is exhaustive over Self, so position() always finds
        // a match; missing means the ALL table is out of sync with the enum.
        let lhs = CanonicalPhase::ALL
            .iter()
            .position(|p| p == self)
            .expect("CanonicalPhase::ALL must list every CanonicalPhase variant");
        let rhs = CanonicalPhase::ALL
            .iter()
            .position(|p| p == other)
            .expect("CanonicalPhase::ALL must list every CanonicalPhase variant");
        lhs.cmp(&rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_phase_ordering_is_total() {
        assert!(CanonicalPhase::PreMergeCI < CanonicalPhase::PromoteProd);
        assert!(CanonicalPhase::PromoteDev > CanonicalPhase::BuildArtifact);
    }
}
