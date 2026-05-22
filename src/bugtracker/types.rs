use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[path = "types_enums.rs"]
mod types_enums;
#[path = "types_records.rs"]
mod types_records;

pub use types_enums::{AttemptStatus, BugPriority, BugSeverity, BugSort, BugStatus};
pub use types_records::{
    BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugEvidenceInput, BugProject,
    BugProjectInput, BugRecord,
};

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
