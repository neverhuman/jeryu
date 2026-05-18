//! Owner: Interactive TUI subsystem — Delivery view collector
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Pure construction; never mutates inputs.
//!
//! Assembles a `DeliverySnapshot` (multiple PRs, each with a canonical-pipeline
//! `WorkflowSnapshot`) from whatever inputs are available.
//!
//! Two synthesized nodes that will be replaced as JeRyu grows:
//!   * `AgentReview { stage }` — auto-passes after `AGENT_REVIEW_AUTO_PASS_DELAY`.
//!   * `AutoMerge` — auto-passes once every pre-merge node has succeeded
//!     (mirrors the user-stated policy: PRs auto-merge when pre-merge CI
//!     passes).

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use super::builder;
use super::model::*;
use crate::release::ReleaseAttemptView;

/// How long the synthesized agent-review node takes to "pass" in demo + live
/// until the real agent review wiring lands.
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

    // ── Phase: Agent review (pre-merge) — synthesized auto-pass ────
    let agent_pre_id = format!("pr{}::agent-review-pre", pr.number);
    let agent_pre_status = agent_review_auto_pass_status(pre_ci_aggregate, pr.created_at, now);
    nodes.push(WorkflowNode {
        id: agent_pre_id.clone(),
        label: "agent code review".into(),
        command: Some("(synthesized) jeryu agent review --pre-merge".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PreMerge,
        },
        status: agent_pre_status,
        required: true,
        deps: pre_test_ids.clone(),
        reason: Some("Synthesized: auto-passes once pre-merge CI is green.".into()),
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

    // ── Phase: Agent review (post-merge) — synthesized auto-pass ───
    let agent_post_id = format!("pr{}::agent-review-post", pr.number);
    let agent_post_status = if pr.merged_into_main {
        agent_review_auto_pass_status(post_ci_aggregate, pr.created_at, now)
    } else {
        WorkflowStatus::Waiting
    };
    nodes.push(WorkflowNode {
        id: agent_post_id.clone(),
        label: "agent regression review".into(),
        command: Some("(synthesized) jeryu agent review --post-merge".into()),
        kind: WorkflowNodeKind::AgentReview {
            stage: AgentStage::PostMerge,
        },
        status: agent_post_status,
        required: false,
        deps: post_test_ids.clone(),
        reason: Some("Stub: auto-passes once post-merge CI is green.".into()),
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

/// Aggregate child-node statuses into a parent gate's status.
fn aggregate_status(tests: &[TestSpec]) -> WorkflowStatus {
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

fn agent_review_auto_pass_status(
    upstream: WorkflowStatus,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> WorkflowStatus {
    match upstream {
        WorkflowStatus::Ran | WorkflowStatus::Cached => {
            if now - created_at >= ChronoDuration::seconds(AGENT_REVIEW_AUTO_PASS_DELAY_SECS) {
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

fn auto_merge_gate_status(pre_ci: WorkflowStatus, agent_pre: WorkflowStatus) -> WorkflowStatus {
    match (pre_ci, agent_pre) {
        (WorkflowStatus::Ran | WorkflowStatus::Cached, WorkflowStatus::Ran) => WorkflowStatus::Ran,
        (WorkflowStatus::Error, _) | (_, WorkflowStatus::Error) | (_, WorkflowStatus::Blocked) => {
            WorkflowStatus::Blocked
        }
        _ => WorkflowStatus::Waiting,
    }
}

fn deployment_status(
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

fn deployment_canary_url(
    dep: &DeploymentProgress,
    release: Option<&ReleaseAttemptView>,
) -> Option<String> {
    dep.canary_url
        .clone()
        .or_else(|| release.and_then(|v| v.canary_public_url.clone()))
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

    let canary_url = release
        .and_then(|v| v.canary_public_url.clone())
        .or_else(|| {
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
        });

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

// ─── Demo factory ────────────────────────────────────────────────────────

/// Build a 5-PR delivery demo showing every interesting state.
pub fn build_demo_delivery() -> DeliverySnapshot {
    let now = Utc::now();

    let prs = vec![
        // PR 1842: mid pre-merge with one failure → blocked.
        PrInput {
            number: 1842,
            title: "feat(api): add cursor pagination to /v2/runs".into(),
            author: "alice".into(),
            head_sha: "a8f42c1".into(),
            created_at: now - ChronoDuration::minutes(14),
            draft: false,
            labels: vec!["api".into(), "needs-review".into()],
            pre_merge_tests: vec![
                test("fmt", "cargo fmt --check", WorkflowStatus::Ran).done(0.6),
                test("clippy", "cargo clippy", WorkflowStatus::Ran).done(8.2),
                test("unit-api", "nextest -- api::", WorkflowStatus::Ran).done(34.1),
                test("unit-tui", "nextest -- tui::", WorkflowStatus::Ran).done(12.0),
                test("build-web", "yarn build", WorkflowStatus::Error)
                    .with_reason("exit 101: type error in src/pages/runs.tsx:42"),
                test("e2e-checkout", "playwright run", WorkflowStatus::Blocked)
                    .with_reason("upstream build-web failed"),
            ],
            merged_into_main: false,
            post_merge_tests: vec![],
            deployment: DeploymentProgress::default(),
        },
        // PR 1841: pre-merge in flight, agent review running.
        PrInput {
            number: 1841,
            title: "fix(tui): pulse selected node border at 1Hz".into(),
            author: "ben".into(),
            head_sha: "9c3a771".into(),
            created_at: now - ChronoDuration::seconds(120),
            draft: false,
            labels: vec!["tui".into()],
            pre_merge_tests: vec![
                test("fmt", "cargo fmt --check", WorkflowStatus::Ran).done(0.4),
                test("clippy", "cargo clippy", WorkflowStatus::Running).at(42, 14),
                test("unit-tui", "nextest -- tui::", WorkflowStatus::Waiting),
            ],
            merged_into_main: false,
            post_merge_tests: vec![],
            deployment: DeploymentProgress::default(),
        },
        // PR 1839: just opened, draft.
        PrInput {
            number: 1839,
            title: "WIP: explore wasmtime sandbox for plugin runtime".into(),
            author: "carla".into(),
            head_sha: "11ee20b".into(),
            created_at: now - ChronoDuration::seconds(40),
            draft: true,
            labels: vec!["wip".into(), "exploration".into()],
            pre_merge_tests: vec![
                test("fmt", "cargo fmt --check", WorkflowStatus::Waiting),
                test("clippy", "cargo clippy", WorkflowStatus::Waiting),
            ],
            merged_into_main: false,
            post_merge_tests: vec![],
            deployment: DeploymentProgress::default(),
        },
        // PR 1837: merged, post-merge CI clean, building artifact.
        PrInput {
            number: 1837,
            title: "feat(release): resume in-flight attempts on startup".into(),
            author: "dani".into(),
            head_sha: "f24eb72".into(),
            created_at: now - ChronoDuration::minutes(45),
            draft: false,
            labels: vec!["release".into()],
            pre_merge_tests: vec![
                test("fmt", "cargo fmt --check", WorkflowStatus::Cached).done(0.1),
                test("clippy", "cargo clippy", WorkflowStatus::Cached).done(0.1),
                test("unit-release", "nextest -- release::", WorkflowStatus::Ran).done(22.4),
            ],
            merged_into_main: true,
            post_merge_tests: vec![
                test("integration", "nextest --test", WorkflowStatus::Ran).done(58.0),
                test("smoke", "scripts/smoke.sh", WorkflowStatus::Ran).done(11.0),
            ],
            deployment: DeploymentProgress {
                build_status: WorkflowStatus::Running,
                build_progress: Some(73),
                local_status: WorkflowStatus::Waiting,
                dev_status: WorkflowStatus::Waiting,
                prod_status: WorkflowStatus::Waiting,
                monitor_status: WorkflowStatus::Waiting,
                canary_url: None,
            },
        },
        // PR 1835: live in canary (dev environment).
        PrInput {
            number: 1835,
            title: "chore(daemon): tune disk sweeper window to 30s".into(),
            author: "ed".into(),
            head_sha: "c521678".into(),
            created_at: now - ChronoDuration::minutes(120),
            draft: false,
            labels: vec!["daemon".into()],
            pre_merge_tests: vec![
                test("fmt", "cargo fmt --check", WorkflowStatus::Cached).done(0.1),
                test("unit-daemon", "nextest -- daemon::", WorkflowStatus::Ran).done(18.0),
            ],
            merged_into_main: true,
            post_merge_tests: vec![
                test("integration", "nextest --test", WorkflowStatus::Ran).done(45.0),
            ],
            deployment: DeploymentProgress {
                build_status: WorkflowStatus::Ran,
                build_progress: Some(100),
                local_status: WorkflowStatus::Ran,
                dev_status: WorkflowStatus::Running,
                prod_status: WorkflowStatus::Waiting,
                monitor_status: WorkflowStatus::Waiting,
                canary_url: Some("https://canary.jeryu.dev/1835".into()),
            },
        },
    ];

    collect_delivery_snapshot(&prs, None)
}

// ─── TestSpec builders ───────────────────────────────────────────────────

fn test(id: &str, command: &str, status: WorkflowStatus) -> TestSpec {
    TestSpec {
        id: id.into(),
        label: id.into(),
        command: command.into(),
        status,
        progress_pct: None,
        eta_secs: None,
        duration_secs: None,
        reason: None,
        critical_path: false,
    }
}

impl TestSpec {
    fn done(mut self, secs: f64) -> Self {
        self.duration_secs = Some(secs);
        self
    }
    fn at(mut self, pct: u16, eta: u64) -> Self {
        self.progress_pct = Some(pct);
        self.eta_secs = Some(eta);
        self
    }
    fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_delivery_renders_all_5_prs() {
        let snap = build_demo_delivery();
        assert_eq!(snap.pull_requests.len(), 5);
        // Numbers preserved & unique.
        let mut nums: Vec<u64> = snap.pull_requests.iter().map(|p| p.number).collect();
        nums.sort();
        nums.dedup();
        assert_eq!(nums.len(), 5);
    }

    #[test]
    fn pr_with_failed_test_is_blocked() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1842)
            .unwrap();
        assert_eq!(pr.status, PrStatus::Blocked);
    }

    #[test]
    fn draft_pr_status_is_draft() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1839)
            .unwrap();
        assert_eq!(pr.status, PrStatus::Draft);
    }

    #[test]
    fn merged_pr_in_canary_is_at_promote_dev() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1835)
            .unwrap();
        assert_eq!(pr.status, PrStatus::Merged);
        assert_eq!(pr.phase, CanonicalPhase::PromoteDev);
    }

    #[test]
    fn fleet_summary_counts_open_and_blocked() {
        let snap = build_demo_delivery();
        let f = &snap.fleet_summary;
        assert_eq!(f.open_prs, 5);
        assert!(f.blocked >= 1);
        assert!(f.canary_in_flight, "PR 1835 is in canary");
    }

    #[test]
    fn agent_review_stub_passes_after_delay() {
        let now = Utc::now();
        let old = now - ChronoDuration::seconds(60);
        let young = now - ChronoDuration::seconds(1);
        assert_eq!(
            agent_review_auto_pass_status(WorkflowStatus::Ran, old, now),
            WorkflowStatus::Ran
        );
        assert_eq!(
            agent_review_auto_pass_status(WorkflowStatus::Ran, young, now),
            WorkflowStatus::Running
        );
    }

    #[test]
    fn agent_review_stub_blocks_on_upstream_error() {
        let now = Utc::now();
        let old = now - ChronoDuration::seconds(60);
        assert_eq!(
            agent_review_auto_pass_status(WorkflowStatus::Error, old, now),
            WorkflowStatus::Blocked
        );
    }

    #[test]
    fn auto_merge_passes_when_all_green() {
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Ran, WorkflowStatus::Ran),
            WorkflowStatus::Ran
        );
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Error, WorkflowStatus::Ran),
            WorkflowStatus::Blocked
        );
        assert_eq!(
            auto_merge_gate_status(WorkflowStatus::Running, WorkflowStatus::Waiting),
            WorkflowStatus::Waiting
        );
    }

    #[test]
    fn canonical_phase_ordering_is_total() {
        assert!(CanonicalPhase::PreMergeCI < CanonicalPhase::PromoteProd);
        assert!(CanonicalPhase::PromoteDev > CanonicalPhase::BuildArtifact);
    }

    #[test]
    fn canonical_pipeline_has_all_phases_for_merged_pr() {
        let snap = build_demo_delivery();
        let pr = snap
            .pull_requests
            .iter()
            .find(|p| p.number == 1835)
            .unwrap();
        let slugs: std::collections::HashSet<_> =
            pr.snapshot.phases.iter().map(|p| p.id.as_str()).collect();
        for canonical in [
            CanonicalPhase::PreMergeCI,
            CanonicalPhase::AgentReviewPreMerge,
            CanonicalPhase::AutoMerge,
            CanonicalPhase::PostMergeCI,
            CanonicalPhase::AgentReviewPostMerge,
            CanonicalPhase::BuildArtifact,
            CanonicalPhase::PromoteLocal,
            CanonicalPhase::PromoteDev,
            CanonicalPhase::PromoteProd,
            CanonicalPhase::MonitorRollback,
        ] {
            assert!(
                slugs.contains(canonical.slug()),
                "missing canonical phase {}",
                canonical.slug()
            );
        }
    }
}
