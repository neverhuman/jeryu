//! Fixed Jeryu repository identities governed by the family authority.

pub(super) const CONTROL_PLANE_PREDECESSOR_TAG: &str = "jeryu-release-ops-v5.0.0-split.6";

pub(super) struct ExpectedRepository {
    pub(super) name: &'static str,
    pub(super) runtime: &'static str,
    pub(super) product_name: Option<&'static str>,
    pub(super) tag: Option<&'static str>,
    pub(super) lfs_required: bool,
}

pub(super) const PRODUCT_AUTHORITIES: [ExpectedRepository; 10] = [
    ExpectedRepository {
        name: "jeryu",
        runtime: "library",
        product_name: None,
        tag: Some("jeryu-v5.0.0-split.0"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-cache",
        runtime: "library",
        product_name: None,
        tag: Some("jeryu-cache-v5.0.0-split.1"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-ci-runner",
        runtime: "shadow-only",
        product_name: None,
        tag: Some("jeryu-ci-runner-v5.0.0-split.0"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-core",
        runtime: "library",
        product_name: None,
        tag: Some("jeryu-core-v5.0.0-split.5"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-deploy",
        runtime: "shadow-only",
        product_name: None,
        tag: Some("jeryu-deploy-v5.0.0-split.3"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-intelligence",
        runtime: "library",
        product_name: None,
        tag: Some("jeryu-intelligence-v5.0.0-split.1"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-jira",
        runtime: "library",
        product_name: Some("Work"),
        tag: Some("jeryu-jira-v5.0.0-split.1"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-tool",
        runtime: "library",
        product_name: None,
        tag: Some("jeryu-tool-v5.1.0-split.7"),
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-tool-finder",
        runtime: "library",
        product_name: None,
        tag: None,
        lfs_required: false,
    },
    ExpectedRepository {
        name: "jeryu-web",
        runtime: "retirement-pending",
        product_name: None,
        tag: Some("jeryu-web-v5.0.0-split.1"),
        lfs_required: false,
    },
];
