//! Persisted bug records, projects, events, and attempt history.
//!
//! Ported from jeryu's `bugtracker/types_records.rs`. `BugAttempt`/`BugAttemptInput`
//! carry `pr_url`; `BugProjectInput.provider_kind`
//! is a generic host descriptor whose value is never defaulted to a forge name.

use serde::{Deserialize, Serialize};

use super::enums::{AttemptStatus, BugPriority, BugSeverity, BugStatus};
use super::types::CanonicalBugReport;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugEvidenceInput {
    pub kind: String,
    pub summary: String,
    pub path: Option<String>,
    pub url: Option<String>,
    pub digest: Option<String>,
    #[serde(default)]
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugProjectInput {
    pub alias: String,
    pub repo_root: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub provider_project_id: Option<String>,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugProject {
    pub alias: String,
    pub repo_root: String,
    pub repo_slug: String,
    pub provider_kind: String,
    pub provider_project_id: Option<String>,
    pub default_branch: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugRecord {
    pub id: String,
    pub title: String,
    pub source_project: String,
    pub target_project: String,
    pub component: Option<String>,
    pub status: BugStatus,
    pub severity: BugSeverity,
    pub priority: BugPriority,
    pub difficulty: u8,
    pub impact: String,
    pub security: bool,
    pub owner: Option<String>,
    pub body: CanonicalBugReport,
    pub created_at: String,
    pub updated_at: String,
    pub attempt_count: i64,
    pub failed_attempt_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugEvent {
    pub id: i64,
    pub bug_id: String,
    pub event_type: String,
    pub actor: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugAttemptInput {
    pub agent: Option<String>,
    pub status: AttemptStatus,
    pub sandbox_path: Option<String>,
    pub branch: Option<String>,
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    pub pr_url: Option<String>,
    pub ci_evidence: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugAttempt {
    pub id: i64,
    pub bug_id: String,
    pub agent: Option<String>,
    pub status: AttemptStatus,
    pub sandbox_path: Option<String>,
    pub branch: Option<String>,
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    pub pr_url: Option<String>,
    pub ci_evidence: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BugDetail {
    pub bug: BugRecord,
    pub events: Vec<BugEvent>,
    pub attempts: Vec<BugAttempt>,
}
