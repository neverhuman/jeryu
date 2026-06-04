use std::path::PathBuf;

use jeryu_agent_auth::AgentToolKind;
use jeryu_agent_stream::AgentControlCommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentWorkRequest {
    pub source: AgentRunSource,
    pub agent: AgentToolKind,
    pub prompt: String,
    pub model: String,
    pub base_ref: String,
    #[serde(default = "default_effort")]
    pub effort: String,
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<String>,
    #[serde(default = "default_branch_suffix")]
    pub branch_suffix: String,
    #[serde(default)]
    pub budget: AgentRunBudget,
    #[serde(default)]
    pub stream: AgentRunStreamOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum AgentRunSource {
    Repo { repo: String },
    LocalPath { local_path: PathBuf },
    Scratch { name: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentRunBudget {
    pub wall_secs: u64,
    pub output_bytes: u64,
}

impl Default for AgentRunBudget {
    fn default() -> Self {
        Self {
            wall_secs: 7200,
            output_bytes: 20_971_520,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentRunStreamOptions {
    #[serde(default = "super::super::workcells_support::default_true")]
    pub required: bool,
}

impl Default for AgentRunStreamOptions {
    fn default() -> Self {
        Self { required: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentControlRequest {
    pub command: AgentControlCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentExportPrRequest {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub(super) struct AgentRunStartResponse {
    pub agent_run_id: String,
    pub workcell_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub status_url: String,
    pub control_topic: String,
    pub tty_topic: String,
    pub export_pr_url: String,
}

pub(super) fn default_effort() -> String {
    "xhigh".to_string()
}

pub(super) fn default_allowed_paths() -> Vec<String> {
    vec![String::new()]
}

pub(super) fn default_branch_suffix() -> String {
    "agent-edit".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_request_deserializes_defaults() {
        let request: AgentWorkRequest = serde_json::from_str(
            r#"{
                "source":{"kind":"scratch","name":"typed-defaults"},
                "agent":"codex",
                "prompt":"fix the failing lane",
                "model":"gpt-5.4-mini",
                "base_ref":"main"
            }"#,
        )
        .unwrap();

        assert!(matches!(
            request.source,
            AgentRunSource::Scratch { name: Some(ref name) } if name == "typed-defaults"
        ));
        assert_eq!(request.effort, "xhigh");
        assert_eq!(request.allowed_paths, vec![String::new()]);
        assert_eq!(request.branch_suffix, "agent-edit");
        assert_eq!(request.budget.wall_secs, 7200);
        assert_eq!(request.budget.output_bytes, 20_971_520);
        assert!(request.stream.required);
    }

    #[test]
    fn control_and_export_requests_decode_boundary_shapes() {
        let control: AgentControlRequest =
            serde_json::from_str(r#"{"command":{"kind":"terminate"}}"#).unwrap();
        assert!(matches!(control.command, AgentControlCommand::Terminate));

        let export: AgentExportPrRequest =
            serde_json::from_str(r#"{"title":"ship agent run","body":"evidence attached"}"#)
                .unwrap();
        assert_eq!(export.title, "ship agent run");
        assert_eq!(export.body.as_deref(), Some("evidence attached"));
    }
}
