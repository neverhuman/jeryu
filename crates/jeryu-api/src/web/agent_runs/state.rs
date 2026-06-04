use std::collections::BTreeMap;
use std::path::PathBuf;

use jeryu_agent_stream::{AgentControlEnvelope, AgentTtyEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AgentRunPhase {
    Queued,
    Starting,
    Running,
    Exited,
    Failed,
    Terminated,
    Exported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AgentRunOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub budget_exceeded: bool,
    pub captured_bytes: usize,
    pub enforcement_level: String,
    pub elapsed_ms: u64,
    pub succeeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct AgentRunSnapshot {
    pub agent_run_id: String,
    pub workcell_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub phase: AgentRunPhase,
    pub agent: String,
    pub model: String,
    pub prompt: String,
    pub source_kind: String,
    pub source_root: PathBuf,
    pub owner: String,
    pub repo: String,
    pub base_ref: String,
    pub branch_suffix: String,
    pub allowed_paths: Vec<String>,
    pub base_sha: String,
    pub head_sha: String,
    pub status_url: String,
    pub control_topic: String,
    pub tty_topic: String,
    pub export_pr_url: String,
    pub events: Vec<AgentTtyEvent>,
    pub controls: Vec<AgentControlEnvelope>,
    pub outcome: Option<AgentRunOutcome>,
    pub error: Option<String>,
    pub export_pull_request_number: Option<u64>,
    pub export_branch: Option<String>,
}

impl AgentRunSnapshot {
    pub(super) fn start_response(&self) -> crate::web::agent_runs::types::AgentRunStartResponse {
        crate::web::agent_runs::types::AgentRunStartResponse {
            agent_run_id: self.agent_run_id.clone(),
            workcell_id: self.workcell_id.clone(),
            runner_id: self.runner_id.clone(),
            runner_epoch: self.runner_epoch,
            status_url: self.status_url.clone(),
            control_topic: self.control_topic.clone(),
            tty_topic: self.tty_topic.clone(),
            export_pr_url: self.export_pr_url.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct AgentRunManager {
    runs: BTreeMap<String, AgentRunSnapshot>,
}

impl AgentRunManager {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(super) fn insert(&mut self, run: AgentRunSnapshot) {
        self.runs.insert(run.agent_run_id.clone(), run);
    }

    pub(super) fn get(&self, run_id: &str) -> Option<AgentRunSnapshot> {
        self.runs.get(run_id).cloned()
    }

    pub(super) fn update(
        &mut self,
        run_id: &str,
        f: impl FnOnce(&mut AgentRunSnapshot),
    ) -> Option<AgentRunSnapshot> {
        let run = self.runs.get_mut(run_id)?;
        f(run);
        Some(run.clone())
    }

    #[cfg(test)]
    pub(super) fn first(&self) -> Option<AgentRunSnapshot> {
        self.runs.values().next().cloned()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jeryu_agent_stream::{AgentControlCommand, AgentControlEnvelope};

    use super::*;

    fn snapshot(run_id: &str) -> AgentRunSnapshot {
        AgentRunSnapshot {
            agent_run_id: run_id.to_string(),
            workcell_id: "wc-1".to_string(),
            runner_id: "runner-1".to_string(),
            runner_epoch: 7,
            phase: AgentRunPhase::Starting,
            agent: "codex".to_string(),
            model: "gpt-5.4-mini".to_string(),
            prompt: "fix".to_string(),
            source_kind: "scratch".to_string(),
            source_root: PathBuf::from("/tmp/workspace"),
            owner: "local".to_string(),
            repo: "scratch".to_string(),
            base_ref: "main".to_string(),
            branch_suffix: "agent-edit".to_string(),
            allowed_paths: vec!["/tmp/workspace".to_string()],
            base_sha: "base".to_string(),
            head_sha: "base".to_string(),
            status_url: format!("/api/v1/agent-runs/{run_id}"),
            control_topic: "jeryu.agent.control.v1".to_string(),
            tty_topic: "jeryu.agent.tty.v1".to_string(),
            export_pr_url: format!("/api/v1/agent-runs/{run_id}/export_pr"),
            events: Vec::new(),
            controls: Vec::new(),
            outcome: None,
            error: None,
            export_pull_request_number: None,
            export_branch: Some("agents/codex/wc-1/agent-edit".to_string()),
        }
    }

    #[test]
    fn manager_insert_get_update_and_start_response_round_trip() {
        let mut manager = AgentRunManager::new();
        let run = snapshot("ar-test");
        let start = run.start_response();
        assert_eq!(start.agent_run_id, "ar-test");
        assert_eq!(start.workcell_id, "wc-1");

        manager.insert(run);
        let fetched = manager.get("ar-test").expect("run exists");
        assert_eq!(fetched.phase, AgentRunPhase::Starting);

        let updated = manager
            .update("ar-test", |run| {
                run.phase = AgentRunPhase::Running;
                run.controls.push(AgentControlEnvelope::new(
                    "ar-test".to_string(),
                    AgentControlCommand::Terminate,
                ));
            })
            .expect("run updates");
        assert_eq!(updated.phase, AgentRunPhase::Running);
        assert_eq!(updated.controls.len(), 1);
        assert!(manager.get("missing").is_none());
    }
}
