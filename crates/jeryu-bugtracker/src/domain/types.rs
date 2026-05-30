//! Canonical bug intake type and its validation rules.
//!
//! Ported from jeryu's `bugtracker/types.rs`. `CanonicalBugReport::validate`
//! is the single gate every bug crosses before it is stored: it enforces the
//! non-empty text fields, the explicit no-secrets confirmation, the difficulty
//! bound, and derives the initial `BugStatus` (NeedsInfo vs NeedsTriage).

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::enums::{BugPriority, BugSeverity, BugStatus};
use super::records::BugEvidenceInput;

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
    /// Validate the report and derive its initial status.
    ///
    /// Returns `NeedsInfo` when neither reproduction steps nor evidence are
    /// present, otherwise `NeedsTriage`. Errors if any required text field is
    /// blank, `no_secrets_confirmed` is false, or `difficulty` is out of range.
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

    /// True when the security/privacy narrative signals a security-sensitive bug.
    ///
    /// Mirrors the heuristic the store applies when persisting a report: an
    /// explicit "no impact" phrasing clears the flag, anything else sets it.
    pub fn is_security_sensitive(&self) -> bool {
        !matches!(
            self.security_privacy.trim().to_ascii_lowercase().as_str(),
            "no" | "none" | "no security impact" | "no privacy impact"
        )
    }
}

fn require_text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{field} is required");
    }
    Ok(())
}
