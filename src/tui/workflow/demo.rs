//! Owner: Interactive TUI subsystem — demo/test data factory
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Test utilities only; not used in production paths.

use chrono::{Duration as ChronoDuration, Utc};

use super::delivery::{DeploymentProgress, PrInput, TestSpec, collect_delivery_snapshot};
use super::model::*;

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

// ─── TestSpec builders ───────────────────────────────────────────────────────

pub(super) fn test(id: &str, command: &str, status: WorkflowStatus) -> TestSpec {
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
    pub(super) fn done(mut self, secs: f64) -> Self {
        self.duration_secs = Some(secs);
        self
    }
    pub(super) fn at(mut self, pct: u16, eta: u64) -> Self {
        self.progress_pct = Some(pct);
        self.eta_secs = Some(eta);
        self
    }
    pub(super) fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.into());
        self
    }
}
