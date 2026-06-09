//! Agent-edit client types.

use serde::{Deserialize, Serialize};

/// Native CLI kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTool {
    /// Codex CLI.
    Codex,
    /// Claude CLI.
    Claude,
    /// Jekko CLI.
    Jekko,
}

impl AgentTool {
    /// Stable label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Jekko => "jekko",
        }
    }
}

/// Agent auth import receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAuthImportReceipt {
    /// Imported tool.
    pub tool: AgentTool,
    /// Jeryu-owned auth dir label.
    pub auth_dir: String,
}

/// Agent auth doctor report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAuthDoctor {
    /// Checked tool.
    pub tool: AgentTool,
    /// Whether imported auth exists.
    pub ok: bool,
}

/// Agent-run start request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRequest {
    /// Repo source as owner/name.
    pub repo: String,
    /// Agent tool.
    pub agent: AgentTool,
    /// Prompt contents.
    pub prompt: String,
    /// Model name.
    pub model: String,
    /// Effort label.
    pub effort: String,
    /// Base ref.
    pub base_ref: String,
}

/// Agent-run status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunStatus {
    /// Run id.
    pub agent_run_id: String,
    /// State label.
    pub state: String,
}

/// Agent control command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentControl {
    /// Send stdin text.
    StdinText { text: String },
    /// Interrupt.
    Interrupt,
    /// Terminate.
    Terminate,
}

/// Agent export request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExportPrRequest {
    /// Run id.
    pub agent_run_id: String,
    /// PR title.
    pub title: String,
    /// Optional PR body.
    pub body: Option<String>,
}

/// Agent export response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentExportPr {
    /// Run id.
    pub agent_run_id: String,
    /// PR URL.
    pub url: String,
}

impl super::InMemoryClient {
    pub(super) fn agent_auth_import_inner(
        &self,
        tool: AgentTool,
    ) -> super::ClientResult<AgentAuthImportReceipt> {
        let mut state = super::lock(&self.state);
        state.agent_auth.insert(tool, true);
        Ok(AgentAuthImportReceipt {
            tool,
            auth_dir: format!("agent-auth/{}", tool.as_str()),
        })
    }

    pub(super) fn agent_auth_doctor_inner(
        &self,
        tool: AgentTool,
    ) -> super::ClientResult<AgentAuthDoctor> {
        let state = super::lock(&self.state);
        Ok(AgentAuthDoctor {
            tool,
            ok: state.agent_auth.get(&tool).copied().unwrap_or(false),
        })
    }

    pub(super) fn agent_run_inner(
        &self,
        request: AgentRunRequest,
    ) -> super::ClientResult<AgentRunStatus> {
        let _ = request;
        Err(super::ClientError::NotWired(
            "agent-run launch requires protected runner PTY, required stream, portable auth, tool doctor, netguard, and enforced sandbox proof".to_string(),
        ))
    }

    pub(super) fn agent_status_inner(&self, run_id: &str) -> super::ClientResult<AgentRunStatus> {
        let state = super::lock(&self.state);
        state
            .agent_runs
            .get(run_id)
            .cloned()
            .ok_or_else(|| super::ClientError::NotFound(format!("agent run {run_id}")))
    }

    pub(super) fn agent_control_inner(
        &self,
        run_id: &str,
        control: AgentControl,
    ) -> super::ClientResult<AgentRunStatus> {
        let _ = control;
        self.agent_status_inner(run_id)
    }

    pub(super) fn agent_export_pr_inner(
        &self,
        request: AgentExportPrRequest,
    ) -> super::ClientResult<AgentExportPr> {
        self.agent_status_inner(&request.agent_run_id)?;
        Ok(AgentExportPr {
            agent_run_id: request.agent_run_id,
            url: "pr://agent-run".to_string(),
        })
    }
}
