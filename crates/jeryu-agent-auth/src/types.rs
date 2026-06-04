use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::AgentAuthError;

/// Native CLI kinds supported by the agent-edit auth bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolKind {
    /// OpenAI Codex CLI.
    Codex,
    /// Claude CLI.
    Claude,
    /// Jekko CLI.
    Jekko,
}

impl AgentToolKind {
    /// Stable lowercase tool id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Jekko => "jekko",
        }
    }
}

impl std::fmt::Display for AgentToolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AgentToolKind {
    type Err = AgentAuthError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "jekko" => Ok(Self::Jekko),
            _ => Err(AgentAuthError::new(
                "agent_auth_invalid_tool",
                "validate requested agent auth tool",
                format!("unknown agent auth tool '{value}'"),
                &[
                    "use one of codex, claude, or jekko",
                    "rerun the CLI with a supported --from-host tool",
                ],
                "docs/testing.md#workcells",
                "rerun cargo test -p jeryu-agent-auth --jobs 40",
            )),
        }
    }
}

/// Options for importing portable auth from a host home/env snapshot.
#[derive(Debug, Clone)]
pub struct AuthImportOptions {
    /// Tool to import.
    pub tool: AgentToolKind,
    /// Host home to inspect. Tests pass a temp dir; production passes the real
    /// operator home but never mounts it into a run.
    pub host_home: PathBuf,
    /// Jeryu data home, usually `~/.local/share/jeryu`.
    pub data_home: PathBuf,
    /// Environment snapshot. Values may contain secrets and are only written to
    /// private files, never included in receipts.
    pub env: BTreeMap<String, String>,
}

/// One copied or materialized auth file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFileReceipt {
    /// Path written by Jeryu.
    pub path: PathBuf,
    /// `sha256:<hex>` digest of the written file.
    pub digest: String,
    /// Unix mode recorded as octal text where available.
    pub mode: String,
}

/// Import receipt. Contains no secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthImportReceipt {
    /// Tool imported.
    pub tool: AgentToolKind,
    /// Jeryu-owned auth directory.
    pub auth_dir: PathBuf,
    /// Files written.
    pub files: Vec<AuthFileReceipt>,
}

/// Per-run materialization receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAuthReceipt {
    /// Tool materialized.
    pub tool: AgentToolKind,
    /// Run home root.
    pub run_home: PathBuf,
    /// Files written under the run home.
    pub files: Vec<AuthFileReceipt>,
}

/// Auth doctor report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthDoctorReport {
    /// Tool checked.
    pub tool: AgentToolKind,
    /// Whether imported portable auth exists.
    pub ok: bool,
    /// Imported files, if present.
    pub files: Vec<AuthFileReceipt>,
}
