//! Owner: bug-tracker domain
//! Proof: `cargo test -p jeryu --lib bugtracker`
//! Invariants: RedlineDB is the only durable backend; canonical reports validate before insert.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod render;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BugSeverity {
    S0,
    S1,
    #[default]
    S2,
    S3,
    S4,
}

impl BugSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::S0 => "S0",
            Self::S1 => "S1",
            Self::S2 => "S2",
            Self::S3 => "S3",
            Self::S4 => "S4",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BugPriority {
    P0,
    P1,
    #[default]
    P2,
    P3,
    P4,
}

impl BugPriority {
    pub fn label(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
            Self::P3 => "P3",
            Self::P4 => "P4",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BugStatus {
    #[default]
    NeedsTriage,
    NeedsInfo,
    Accepted,
    Ready,
    InProgress,
    Blocked,
    FixProposed,
    Reviewing,
    Verifying,
    Done,
    Duplicate,
    Invalid,
    CannotReproduce,
    WontDo,
}

impl BugStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsTriage => "needs_triage",
            Self::NeedsInfo => "needs_info",
            Self::Accepted => "accepted",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::FixProposed => "fix_proposed",
            Self::Reviewing => "reviewing",
            Self::Verifying => "verifying",
            Self::Done => "done",
            Self::Duplicate => "duplicate",
            Self::Invalid => "invalid",
            Self::CannotReproduce => "cannot_reproduce",
            Self::WontDo => "wont_do",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Duplicate | Self::Invalid | Self::CannotReproduce | Self::WontDo
        )
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "needs_triage" => Ok(Self::NeedsTriage),
            "needs_info" => Ok(Self::NeedsInfo),
            "accepted" => Ok(Self::Accepted),
            "ready" => Ok(Self::Ready),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "fix_proposed" => Ok(Self::FixProposed),
            "reviewing" => Ok(Self::Reviewing),
            "verifying" => Ok(Self::Verifying),
            "done" => Ok(Self::Done),
            "duplicate" => Ok(Self::Duplicate),
            "invalid" => Ok(Self::Invalid),
            "cannot_reproduce" => Ok(Self::CannotReproduce),
            "wont_do" => Ok(Self::WontDo),
            other => bail!("unknown bug status '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BugSort {
    #[default]
    Rank,
    Severity,
    Priority,
    Difficulty,
    Ready,
    Updated,
    Attempts,
}

impl BugSort {
    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "rank" => Ok(Self::Rank),
            "severity" => Ok(Self::Severity),
            "priority" => Ok(Self::Priority),
            "difficulty" => Ok(Self::Difficulty),
            "ready" => Ok(Self::Ready),
            "updated" => Ok(Self::Updated),
            "attempts" => Ok(Self::Attempts),
            other => bail!("unknown bug sort '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Pending,
    Started,
    Failed,
    FixProposed,
    Verified,
    Abandoned,
}

impl AttemptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Started => "started",
            Self::Failed => "failed",
            Self::FixProposed => "fix_proposed",
            Self::Verified => "verified",
            Self::Abandoned => "abandoned",
        }
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input {
            "pending" => Ok(Self::Pending),
            "started" => Ok(Self::Started),
            "failed" => Ok(Self::Failed),
            "fix_proposed" => Ok(Self::FixProposed),
            "verified" => Ok(Self::Verified),
            "abandoned" => Ok(Self::Abandoned),
            other => bail!("unknown attempt status '{other}'"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalBugReport {
    pub target_project: String,
    pub source_project: String,
    pub title: String,
    pub component: Option<String>,
    pub current_behavior: String,
    pub expected_behavior: String,
    pub environment: String,
    pub frequency: String,
    pub impact: String,
    pub security_privacy: String,
    pub no_secrets_confirmed: bool,
    #[serde(default)]
    pub reproduction_steps: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<BugEvidenceInput>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub severity: BugSeverity,
    #[serde(default)]
    pub priority: BugPriority,
    #[serde(default = "default_difficulty")]
    pub difficulty: u8,
}

fn default_difficulty() -> u8 {
    3
}

impl CanonicalBugReport {
    pub fn validate(&self) -> Result<BugStatus> {
        require_text("target_project", &self.target_project)?;
        require_text("source_project", &self.source_project)?;
        require_text("title", &self.title)?;
        require_text("current_behavior", &self.current_behavior)?;
        require_text("expected_behavior", &self.expected_behavior)?;
        require_text("environment", &self.environment)?;
        require_text("frequency", &self.frequency)?;
        require_text("impact", &self.impact)?;
        require_text("security_privacy", &self.security_privacy)?;
        if !self.no_secrets_confirmed {
            bail!("no_secrets_confirmed must be true before a bug can be stored");
        }
        if !(1..=5).contains(&self.difficulty) {
            bail!("difficulty must be between 1 and 5");
        }
        if self.reproduction_steps.is_empty() && self.evidence.is_empty() {
            return Ok(BugStatus::NeedsInfo);
        }
        Ok(BugStatus::NeedsTriage)
    }
}

fn require_text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    Ok(())
}

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

pub fn generate_bug_id(report: &CanonicalBugReport, now: DateTime<Utc>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(report.target_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.source_project.as_bytes());
    hasher.update(b"\0");
    hasher.update(report.title.as_bytes());
    hasher.update(b"\0");
    hasher.update(now.timestamp_nanos_opt().unwrap_or_default().to_string());
    let digest = hasher.finalize();
    format!("bug-{}", hex::encode(&digest[..5]))
}

pub fn branch_name(bug_id: &str, title: &str) -> String {
    let slug = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "bug/{bug_id}-{}",
        if slug.is_empty() { "fix" } else { &slug }
    )
}

pub fn validate_transition(from: BugStatus, to: BugStatus) -> Result<()> {
    if from.is_terminal() && from != to {
        bail!(
            "terminal bug status {} cannot transition to {}",
            from.as_str(),
            to.as_str()
        );
    }
    Ok(())
}

pub fn ranking_key(bug: &BugRecord) -> (u8, u8, u8, u8, i64, String) {
    let ready_rank = match bug.status {
        BugStatus::Ready => 0,
        BugStatus::Accepted | BugStatus::NeedsTriage => 1,
        BugStatus::Blocked | BugStatus::NeedsInfo => 3,
        status if status.is_terminal() => 5,
        _ => 2,
    };
    (
        bug.severity as u8,
        bug.priority as u8,
        ready_rank,
        bug.difficulty,
        -bug.failed_attempt_count,
        bug.updated_at.clone(),
    )
}

pub fn parse_report_json(input: &str) -> Result<CanonicalBugReport> {
    serde_json::from_str(input).context("parse canonical bug report JSON")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> CanonicalBugReport {
        CanonicalBugReport {
            target_project: "redlinedb".into(),
            source_project: "veox".into(),
            title: "redline adapter loses writes".into(),
            component: Some("adapter".into()),
            current_behavior: "writes disappear".into(),
            expected_behavior: "writes persist".into(),
            environment: "local".into(),
            frequency: "always".into(),
            impact: "blocks local agent state".into(),
            security_privacy: "no security impact".into(),
            no_secrets_confirmed: true,
            reproduction_steps: vec!["submit write".into(), "restart".into()],
            evidence: Vec::new(),
            acceptance_criteria: vec!["write survives restart".into()],
            severity: BugSeverity::S1,
            priority: BugPriority::P1,
            difficulty: 3,
        }
    }

    #[test]
    fn validation_requires_no_secrets_confirmation() {
        let mut r = report();
        r.no_secrets_confirmed = false;
        assert!(r.validate().is_err());
    }

    #[test]
    fn validation_lands_missing_repro_in_needs_info() {
        let mut r = report();
        r.reproduction_steps.clear();
        assert_eq!(r.validate().unwrap(), BugStatus::NeedsInfo);
    }

    #[test]
    fn generated_ids_use_bug_prefix_and_hash_length() {
        let id = generate_bug_id(&report(), Utc::now());
        assert!(id.starts_with("bug-"));
        assert_eq!(id.len(), 14);
    }

    #[test]
    fn terminal_status_blocks_reopen() {
        assert!(validate_transition(BugStatus::Done, BugStatus::Ready).is_err());
        assert!(validate_transition(BugStatus::Ready, BugStatus::Done).is_ok());
    }
}
