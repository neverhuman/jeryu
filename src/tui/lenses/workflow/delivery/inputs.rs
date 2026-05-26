//! Delivery collector input types.

use chrono::{DateTime, Utc};

use crate::tui::lenses::workflow::model::WorkflowStatus;

pub const AGENT_REVIEW_AUTO_PASS_DELAY_SECS: i64 = 5;

#[derive(Debug, Clone)]
pub struct PrInput {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
    pub draft: bool,
    pub labels: Vec<String>,
    pub pre_merge_tests: Vec<TestSpec>,
    pub merged_into_main: bool,
    pub post_merge_tests: Vec<TestSpec>,
    pub deployment: DeploymentProgress,
    pub repo_alias: Option<String>,
    pub repo_slug: Option<String>,
}

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
