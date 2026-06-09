use serde::{Deserialize, Serialize};

use crate::AgentToolKind;

/// Required repair shape for agent-auth failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAuthRepair {
    /// What operation failed.
    pub purpose: String,
    /// Why it failed.
    pub reason: String,
    /// Operator-actionable fixes.
    pub common_fixes: Vec<String>,
    /// Owning documentation URL.
    pub docs_url: String,
    /// Local rerun or repair hint.
    pub repair_hint: String,
}

/// Typed agent-auth error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {}", repair.reason)]
pub struct AgentAuthError {
    /// Stable machine code.
    pub code: String,
    /// Required repair body.
    pub repair: Box<AgentAuthRepair>,
}

impl AgentAuthError {
    pub(crate) fn new(
        code: &'static str,
        purpose: impl Into<String>,
        reason: impl Into<String>,
        common_fixes: &[&str],
        docs_url: &'static str,
        repair_hint: &'static str,
    ) -> Self {
        Self {
            code: code.to_string(),
            repair: Box::new(AgentAuthRepair {
                purpose: purpose.into(),
                reason: reason.into(),
                common_fixes: common_fixes.iter().map(|fix| (*fix).to_string()).collect(),
                docs_url: docs_url.to_string(),
                repair_hint: repair_hint.to_string(),
            }),
        }
    }
}

pub(crate) fn missing_auth(tool: AgentToolKind) -> AgentAuthError {
    AgentAuthError::new(
        "agent_auth_missing",
        format!("import portable {tool} auth"),
        format!("no portable {tool} auth was found"),
        &[
            "run jeryu agent auth import --from-host for the requested tool",
            "provide a portable token environment variable before importing",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-auth --jobs 40",
    )
}

pub(crate) fn fs_error(error: std::io::Error) -> AgentAuthError {
    AgentAuthError::new(
        "agent_auth_io",
        "copy portable agent auth",
        error.to_string(),
        &[
            "check permissions on the source and Jeryu data directories",
            "retry the auth import after fixing filesystem access",
        ],
        "docs/testing.md#workcells",
        "rerun cargo test -p jeryu-agent-auth --jobs 40",
    )
}
