//! Owner: Interactive TUI subsystem — Delivery demo factory + post-merge fixtures
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::delivery`
//! Invariants: Test/demo utilities only; not used in production paths.

use chrono::{Duration as ChronoDuration, Utc};

use super::ci::test;
use super::{DeploymentProgress, PrInput, collect_delivery_snapshot};
use crate::tui::workflow::model::{DeliverySnapshot, WorkflowStatus};

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
            repo_alias: Some("nht".into()),
            repo_slug: Some("neverhuman/veox-nht".into()),
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
            repo_alias: Some("shared".into()),
            repo_slug: Some("neverhuman/veox-shared".into()),
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
            repo_alias: Some("warp".into()),
            repo_slug: Some("neverhuman/veox-warp".into()),
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
            repo_alias: Some("nht".into()),
            repo_slug: Some("neverhuman/veox-nht".into()),
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
            repo_alias: Some("shared".into()),
            repo_slug: Some("neverhuman/veox-shared".into()),
        },
    ];

    collect_delivery_snapshot(&prs, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::model::{CanonicalPhase, PrStatus};

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
