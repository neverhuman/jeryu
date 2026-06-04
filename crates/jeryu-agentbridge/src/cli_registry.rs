//! Coding-agent CLI registry: which agent CLI to run for a requested model, and
//! how to build its non-interactive (headless) command line.
//!
//! The in-cell agent driver runs exactly one coding-agent CLI - `claude`,
//! `codex`, or `jekko` - jailed inside a cell checkout. Each CLI exposes a
//! different headless surface (flags, model/effort selection, prompt delivery),
//! so this module is the single place that maps a requested `model` string to a
//! [`AgentCli`] and produces the concrete [`LaunchPlan`] (program, argv, extra
//! env, and whether the prompt is delivered on stdin).
//!
//! This module is intentionally pure: it builds the command line but never
//! spawns anything and has no knowledge of the sandbox. The driver feeds the
//! resulting [`LaunchPlan`] into a `CommandSpec` and the jail does the rest.

use std::collections::BTreeMap;
use std::fmt;

/// A supported coding-agent CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentCli {
    /// Anthropic Claude Code CLI (`claude`).
    Claude,
    /// OpenAI Codex CLI (`codex`).
    Codex,
    /// Jekko - Rust-native multi-provider coding agent (`jekko`).
    Jekko,
}

impl AgentCli {
    /// Stable lowercase label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Jekko => "jekko",
        }
    }

    /// The default program name resolved on PATH when no explicit path is given.
    #[must_use]
    pub fn default_program(self) -> &'static str {
        self.as_str()
    }
}

impl fmt::Display for AgentCli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AgentCli {
    type Err = CliRegistryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "jekko" => Ok(Self::Jekko),
            other => Err(CliRegistryError::UnknownAgent(other.to_string())),
        }
    }
}

/// Reasoning-effort level requested for the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    /// Lowest effort.
    Low,
    /// Medium effort.
    Medium,
    /// High effort.
    High,
    /// Extra-high effort.
    XHigh,
    /// Maximum effort.
    Max,
}

impl Effort {
    /// Canonical lowercase token shared across the CLIs that accept named efforts.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

impl std::str::FromStr for Effort {
    type Err = CliRegistryError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" | "med" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" | "x-high" | "extra-high" => Ok(Self::XHigh),
            "max" | "maximum" => Ok(Self::Max),
            other => Err(CliRegistryError::UnknownEffort(other.to_string())),
        }
    }
}

/// The model + effort + optional provider the caller asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelSelect {
    /// Model identifier as requested.
    pub model: String,
    /// Optional reasoning effort.
    pub effort: Option<Effort>,
    /// Optional provider hint, used by multi-provider CLIs.
    pub provider: Option<String>,
}

impl ModelSelect {
    /// Convenience constructor for a model with no effort/provider.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            effort: None,
            provider: None,
        }
    }

    /// Builder: set the reasoning effort.
    #[must_use]
    pub fn with_effort(mut self, effort: Effort) -> Self {
        self.effort = Some(effort);
        self
    }

    /// Builder: set the provider hint.
    #[must_use]
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }
}

/// A concrete, ready-to-spawn headless command for an agent CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// Program to exec.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Extra environment entries to merge on top of the sandbox base env.
    pub extra_env: BTreeMap<String, String>,
    /// When true, the task prompt is written to the child's stdin.
    pub prompt_on_stdin: bool,
}

/// Errors raised while routing a model or building a launch plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliRegistryError {
    /// No agent CLI claims this model string.
    UnroutableModel(String),
    /// The agent name was not one of `claude`/`codex`/`jekko`.
    UnknownAgent(String),
    /// The effort token was not recognised.
    UnknownEffort(String),
}

impl fmt::Display for CliRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnroutableModel(m) => write!(
                f,
                "agent_cli_unroutable_model: no agent CLI handles model '{m}' (expected a gpt-*/o*/codex* (codex), claude-*/opus/sonnet/haiku (claude), or jekko:* / provider/model (jekko) form)"
            ),
            Self::UnknownAgent(a) => {
                write!(f, "agent_cli_unknown: '{a}' is not claude|codex|jekko")
            }
            Self::UnknownEffort(e) => {
                write!(
                    f,
                    "agent_cli_unknown_effort: '{e}' is not low|medium|high|xhigh|max"
                )
            }
        }
    }
}

impl std::error::Error for CliRegistryError {}

/// Route a requested model string to the agent CLI that should run it.
pub fn route_model(model: &str) -> Result<AgentCli, CliRegistryError> {
    let m = model.trim().to_ascii_lowercase();
    if m.is_empty() {
        return Err(CliRegistryError::UnroutableModel(model.to_string()));
    }
    if let Some(rest) = m.strip_prefix("jekko:") {
        let _ = rest;
        return Ok(AgentCli::Jekko);
    }
    if m.starts_with("gpt")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.contains("codex")
    {
        return Ok(AgentCli::Codex);
    }
    if m.starts_with("claude") || m.contains("opus") || m.contains("sonnet") || m.contains("haiku")
    {
        return Ok(AgentCli::Claude);
    }
    if m.contains('/') {
        return Ok(AgentCli::Jekko);
    }
    Err(CliRegistryError::UnroutableModel(model.to_string()))
}

/// Builds the headless command line for one agent CLI.
pub trait CliAdapter {
    /// Which CLI this adapter drives.
    fn cli(&self) -> AgentCli;

    /// Build the headless launch plan.
    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeAdapter;

impl CliAdapter for ClaudeAdapter {
    fn cli(&self) -> AgentCli {
        AgentCli::Claude
    }

    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan {
        let mut args = vec!["--print".to_string()];
        if stream_json {
            args.push("--output-format".to_string());
            args.push("stream-json".to_string());
        }
        args.push("--verbose".to_string());
        args.push("--model".to_string());
        args.push(model.model.clone());
        if let Some(effort) = model.effort {
            args.push("--effort".to_string());
            args.push(effort.as_str().to_string());
        }
        LaunchPlan {
            program: program.to_string(),
            args,
            extra_env: BTreeMap::new(),
            prompt_on_stdin: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAdapter;

impl CliAdapter for CodexAdapter {
    fn cli(&self) -> AgentCli {
        AgentCli::Codex
    }

    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan {
        let mut args = vec![
            "exec".to_string(),
            "-m".to_string(),
            model.model.clone(),
            "-c".to_string(),
            format!(
                "model_reasoning_effort=\"{}\"",
                model.effort.unwrap_or(Effort::XHigh).as_str()
            ),
            "-s".to_string(),
            "workspace-write".to_string(),
        ];
        if stream_json {
            args.push("--json".to_string());
        }
        LaunchPlan {
            program: program.to_string(),
            args,
            extra_env: BTreeMap::new(),
            prompt_on_stdin: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct JekkoAdapter;

impl CliAdapter for JekkoAdapter {
    fn cli(&self) -> AgentCli {
        AgentCli::Jekko
    }

    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan {
        let mut args = vec![
            "run".to_string(),
            "--provider".to_string(),
            model
                .provider
                .clone()
                .unwrap_or_else(|| "anthropic".to_string()),
            "--model".to_string(),
            model.model.clone(),
            "--ephemeral".to_string(),
            "--headless".to_string(),
        ];
        if stream_json {
            args.push("--json".to_string());
        }
        LaunchPlan {
            program: program.to_string(),
            args,
            extra_env: BTreeMap::new(),
            prompt_on_stdin: true,
        }
    }
}

/// Return an adapter for a selected CLI.
#[must_use]
pub fn adapter_for(cli: AgentCli) -> &'static dyn CliAdapter {
    match cli {
        AgentCli::Claude => &ClaudeAdapter,
        AgentCli::Codex => &CodexAdapter,
        AgentCli::Jekko => &JekkoAdapter,
    }
}

/// Route a model, then build the launch plan using the matching adapter.
pub fn plan_launch(
    program: &str,
    model: &ModelSelect,
    stream_json: bool,
) -> Result<(AgentCli, LaunchPlan), CliRegistryError> {
    let cli = route_model(&model.model)?;
    Ok((
        cli,
        adapter_for(cli).build_launch(program, model, stream_json),
    ))
}
