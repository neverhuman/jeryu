//! Owner: Release Pipeline (evidence capsules)
//! Proof: `cargo test -p jeryu -- release::capsule`
//! Invariants: Capsules are write-once, structured, and self-describing.
//!
//! A capsule is the Evidence Capsule referenced in JeRyu's mission docs and
//! in tip5.txt §3 "Agent self-review". It is the durable, machine-readable
//! summary an agent produces alongside a draft PR.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCapsule {
    pub schema_version: String,
    pub agent_id: String,
    pub task: String,
    pub issue: Option<u64>,
    pub branch: String,
    pub risk_tier: u8,
    pub change_type: String,
    pub base_sha: Option<String>,
    pub head_sha: Option<String>,
    pub vti_receipt: Option<String>,
    pub commands_run: Vec<String>,
    pub skipped_subsystems: Vec<String>,
    pub confidence: String,
    pub rollback_plan: String,
    pub created_at: String,
}

impl EvidenceCapsule {
    pub fn new(
        agent_id: impl Into<String>,
        task: impl Into<String>,
        branch: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "1.0.0".into(),
            agent_id: agent_id.into(),
            task: task.into(),
            issue: None,
            branch: branch.into(),
            risk_tier: 1,
            change_type: "leaf-bugfix".into(),
            base_sha: None,
            head_sha: None,
            vti_receipt: None,
            commands_run: vec![],
            skipped_subsystems: vec![],
            confidence: "medium".into(),
            rollback_plan: "revert PR + patch release".into(),
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn write(&self, dir: PathBuf) -> Result<PathBuf> {
        fs::create_dir_all(&dir)
            .with_context(|| format!("create capsule dir {}", dir.display()))?;
        let path = dir.join("capsule.json");
        let body = serde_json::to_string_pretty(self)?;
        fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
        Ok(path)
    }

    /// Markdown body suitable for a draft PR body.
    pub fn render_pr_body(&self) -> String {
        let issue_line = match self.issue {
            Some(n) => format!("Closes #{n}"),
            None => "<!-- no issue linked -->".to_string(),
        };
        format!(
            "## Description\n\
             {task}\n\n\
             ## Motivation and Context\n\
             {issue_line}\n\n\
             ## Agent disclosure\n\
             - Authoring agent: {agent}\n\
             - JeRyu session / evidence id: {capsule_id}\n\n\
             ## Risk tier\n\
             - Tier {tier}\n\n\
             ## Change type\n\
             - {change_type}\n\n\
             ## Proof\n\
             - VTI receipt: {vti}\n\
             - Commands run: {cmds}\n\
             - Skipped subsystems: {skipped}\n\
             - Confidence: {confidence}\n\n\
             ## Rollback plan\n\
             {rollback}\n",
            task = self.task,
            issue_line = issue_line,
            agent = self.agent_id,
            capsule_id = self.created_at,
            tier = self.risk_tier,
            change_type = self.change_type,
            vti = self.vti_receipt.as_deref().unwrap_or("(none)"),
            cmds = if self.commands_run.is_empty() {
                "(none yet)".into()
            } else {
                self.commands_run.join(", ")
            },
            skipped = if self.skipped_subsystems.is_empty() {
                "(none)".into()
            } else {
                self.skipped_subsystems.join(", ")
            },
            confidence = self.confidence,
            rollback = self.rollback_plan,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsule_has_expected_defaults() {
        let c = EvidenceCapsule::new("claude", "fix x", "agent/claude/1-fix-x");
        assert_eq!(c.schema_version, "1.0.0");
        assert_eq!(c.risk_tier, 1);
        assert_eq!(c.change_type, "leaf-bugfix");
        assert!(c.created_at.contains('T'));
    }

    #[test]
    fn write_creates_capsule_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let c = EvidenceCapsule::new("a", "t", "b");
        let path = c.write(tmp.path().to_path_buf()).expect("write");
        assert!(path.exists());
        assert!(path.file_name().unwrap() == "capsule.json");
    }

    #[test]
    fn pr_body_includes_agent_and_rollback() {
        let mut c = EvidenceCapsule::new("claude", "fix x", "agent/claude/1-fix-x");
        c.issue = Some(42);
        c.rollback_plan = "feature flag jeryu_release_v2".into();
        let body = c.render_pr_body();
        assert!(body.contains("Closes #42"));
        assert!(body.contains("claude"));
        assert!(body.contains("feature flag jeryu_release_v2"));
    }
}
