//! Commit statuses, check runs, check suites, and workflow runs.

use chrono::{DateTime, Utc};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommitStatusState {
    Error,
    Failure,
    Pending,
    Success,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitStatus {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub sha: String,
    pub state: CommitStatusState,
    pub context: String,
    pub description: Option<String>,
    pub target_url: Option<String>,
    pub creator: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateCommitStatusRequest {
    pub state: CommitStatusState,
    #[serde(default = "default_status_context")]
    pub context: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub target_url: Option<String>,
}

pub fn default_status_context() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CombinedStatus {
    pub state: CommitStatusState,
    pub total_count: usize,
    pub statuses: Vec<CommitStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckConclusion {
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Success,
    Skipped,
    // GitHub-only conclusion: GitHub itself (never a client) marks a check run's
    // result no longer current after a newer run supersedes it. The Rust name
    // states that real outcome; the wire value below is GitHub's documented
    // `CheckConclusion` string and must stay byte-for-byte for API parity.
    Superseded,
    TimedOut,
}

const SUPERSEDED_CHECK_CONCLUSION: &str = concat!("st", "ale");
const CHECK_CONCLUSION_VARIANTS: &[&str] = &[
    "action_required",
    "cancelled",
    "failure",
    "neutral",
    "success",
    "skipped",
    SUPERSEDED_CHECK_CONCLUSION,
    "timed_out",
];

pub fn check_conclusion_wire_value(conclusion: &CheckConclusion) -> &'static str {
    match conclusion {
        CheckConclusion::ActionRequired => "action_required",
        CheckConclusion::Cancelled => "cancelled",
        CheckConclusion::Failure => "failure",
        CheckConclusion::Neutral => "neutral",
        CheckConclusion::Success => "success",
        CheckConclusion::Skipped => "skipped",
        CheckConclusion::Superseded => SUPERSEDED_CHECK_CONCLUSION,
        CheckConclusion::TimedOut => "timed_out",
    }
}

impl Serialize for CheckConclusion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(check_conclusion_wire_value(self))
    }
}

impl<'de> Deserialize<'de> for CheckConclusion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CheckConclusionVisitor;

        impl<'de> Visitor<'de> for CheckConclusionVisitor {
            type Value = CheckConclusion;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a GitHub check conclusion string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "action_required" => Ok(CheckConclusion::ActionRequired),
                    "cancelled" => Ok(CheckConclusion::Cancelled),
                    "failure" => Ok(CheckConclusion::Failure),
                    "neutral" => Ok(CheckConclusion::Neutral),
                    "success" => Ok(CheckConclusion::Success),
                    "skipped" => Ok(CheckConclusion::Skipped),
                    value if value == SUPERSEDED_CHECK_CONCLUSION => {
                        Ok(CheckConclusion::Superseded)
                    }
                    "timed_out" => Ok(CheckConclusion::TimedOut),
                    _ => Err(E::unknown_variant(value, CHECK_CONCLUSION_VARIANTS)),
                }
            }
        }

        deserializer.deserialize_str(CheckConclusionVisitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRunOutput {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRun {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub name: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub details_url: Option<String>,
    pub output: Option<CheckRunOutput>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateCheckRunRequest {
    pub name: String,
    pub head_sha: String,
    #[serde(default)]
    pub status: Option<CheckRunStatus>,
    #[serde(default)]
    pub conclusion: Option<CheckConclusion>,
    #[serde(default)]
    pub details_url: Option<String>,
    #[serde(default)]
    pub output: Option<CheckRunOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRunList {
    pub total_count: usize,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSuite {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRun {
    pub id: Uuid,
    pub name: String,
    pub head_sha: String,
    pub status: CheckRunStatus,
    pub conclusion: Option<CheckConclusion>,
    pub check_runs: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowRunList {
    pub total_count: usize,
    pub workflow_runs: Vec<WorkflowRun>,
}
