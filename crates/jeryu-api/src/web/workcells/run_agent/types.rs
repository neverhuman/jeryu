use std::collections::BTreeMap;
use std::path::PathBuf;

use jeryu_agentbridge::driver::{AgentEvent, AgentRunResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentWorkcellRunRequest {
    pub workcell_id: String,
    pub runner_epoch: u64,
    #[serde(default)]
    pub repo_root: Option<PathBuf>,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub output_budget_bytes: Option<usize>,
    #[serde(default = "crate::web::workcells_support::default_true")]
    pub require_cgroup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentWorkcellRunEvent {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub budget_exceeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentWorkcellRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub budget_exceeded: bool,
    pub captured_bytes: usize,
    pub enforcement_level: String,
    pub elapsed_ms: u64,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AgentWorkcellRunResponse {
    pub workcell_id: String,
    pub runner_epoch: u64,
    pub repo_root: PathBuf,
    pub events: Vec<AgentWorkcellRunEvent>,
    pub outcome: AgentWorkcellRunOutcome,
}

impl From<AgentEvent> for AgentWorkcellRunEvent {
    fn from(value: AgentEvent) -> Self {
        match value {
            AgentEvent::Started { pid } => Self {
                kind: "started".to_string(),
                stream: None,
                text: None,
                pid: Some(pid),
                used: None,
                limit: None,
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Stdout(text) => line_event("stdout", text),
            AgentEvent::Stderr(text) => line_event("stderr", text),
            AgentEvent::Budget { used, limit } => Self {
                kind: "budget".to_string(),
                stream: None,
                text: None,
                pid: None,
                used: Some(used),
                limit: Some(limit),
                exit_code: None,
                timed_out: false,
                budget_exceeded: false,
            },
            AgentEvent::Finished {
                exit_code,
                timed_out,
                budget_exceeded,
            } => Self {
                kind: "finished".to_string(),
                stream: None,
                text: None,
                pid: None,
                used: None,
                limit: None,
                exit_code,
                timed_out,
                budget_exceeded,
            },
        }
    }
}

impl From<AgentRunResult> for AgentWorkcellRunOutcome {
    fn from(value: AgentRunResult) -> Self {
        let succeeded = value.succeeded();
        Self {
            exit_code: value.exit_code,
            timed_out: value.timed_out,
            budget_exceeded: value.budget_exceeded,
            captured_bytes: value.captured_bytes,
            enforcement_level: value.enforcement_level,
            elapsed_ms: u64::try_from(value.elapsed.as_millis()).unwrap_or(u64::MAX),
            succeeded,
        }
    }
}

fn line_event(stream: &str, text: String) -> AgentWorkcellRunEvent {
    AgentWorkcellRunEvent {
        kind: "line".to_string(),
        stream: Some(stream.to_string()),
        text: Some(text),
        pid: None,
        used: None,
        limit: None,
        exit_code: None,
        timed_out: false,
        budget_exceeded: false,
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}
