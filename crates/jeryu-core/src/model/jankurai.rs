//! Jankurai audit-score records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A recorded jankurai audit of one repository commit.
///
/// `score` is `None` when the auditor could not score the tree
/// (`decision == "tool-failed"`, e.g. non-Rust repos) — a recorded fact, not
/// an error. The full `repo-score.json` rides along verbatim as a raw JSON
/// string so derives stay `Eq` and forensics never lose data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JankuraiScore {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub commit_sha: String,
    pub score: Option<u32>,
    pub hard_findings: u32,
    pub decision: String,
    pub caps_applied: Vec<String>,
    pub report_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Ingest payload for one audit outcome, as POSTed by CI lanes and the
/// backfill sweep (`ops/jankurai/backfill-repo-scores.sh`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RecordJankuraiScoreRequest {
    pub branch: String,
    pub commit_sha: String,
    #[serde(default)]
    pub score: Option<u32>,
    #[serde(default)]
    pub hard_findings: Option<u32>,
    pub decision: String,
    #[serde(default)]
    pub caps_applied: Vec<String>,
    /// Full repo-score.json, stored verbatim.
    #[serde(default)]
    pub report: Option<serde_json::Value>,
    /// Auditor exit code when no report was produced (tool-failed runs).
    #[serde(default)]
    pub tool_exit: Option<i64>,
}
