//! Coding-agent CLI registry: model routing and headless launch plans.
//!
//! The in-cell agent driver runs a single coding-agent CLI jailed inside a cell
//! checkout. Each supported CLI exposes a different headless surface, so this
//! module is the pure, testable mapping from a requested model string to the
//! concrete program/argv/env plan. It does not spawn processes or know about
//! sandbox paths.

use std::collections::BTreeMap;
use std::fmt;

/// A supported coding-agent CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentCli {
    /// Anthropic Claude Code CLI.
    Claude,
    /// OpenAI Codex CLI.
    Codex,
    /// Jekko multi-provider coding agent.
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

    /// Default program name resolved on `PATH`.
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
    /// Canonical lowercase token.
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

/// The model, optional effort, and optional provider requested by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelSelect {
    /// Model identifier.
    pub model: String,
    /// Optional reasoning effort.
    pub effort: Option<Effort>,
    /// Optional provider hint for multi-provider CLIs.
    pub provider: Option<String>,
}

impl ModelSelect {
    /// Build a model selection with no effort/provider.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            effort: None,
            provider: None,
        }
    }

    /// Set reasoning effort.
    #[must_use]
    pub fn with_effort(mut self, effort: Effort) -> Self {
        self.effort = Some(effort);
        self
    }

    /// Set provider hint.
    #[must_use]
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = Some(provider.into());
        self
    }
}

/// A concrete headless command for an agent CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    /// Program to execute.
    pub program: String,
    /// Arguments; never include the task prompt when `prompt_on_stdin` is true.
    pub args: Vec<String>,
    /// Extra environment entries.
    pub extra_env: BTreeMap<String, String>,
    /// Deliver the task prompt through stdin.
    pub prompt_on_stdin: bool,
}

/// Errors raised while routing a model or building a launch plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliRegistryError {
    /// No agent CLI claims this model string.
    UnroutableModel(String),
    /// Unknown agent name.
    UnknownAgent(String),
    /// Unknown effort token.
    UnknownEffort(String),
}

impl fmt::Display for CliRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnroutableModel(model) => write!(
                f,
                "agent_cli_unroutable_model: no agent CLI handles model '{model}'"
            ),
            Self::UnknownAgent(agent) => {
                write!(f, "agent_cli_unknown: '{agent}' is not claude|codex|jekko")
            }
            Self::UnknownEffort(effort) => {
                write!(
                    f,
                    "agent_cli_unknown_effort: '{effort}' is not low|medium|high|xhigh|max"
                )
            }
        }
    }
}

impl std::error::Error for CliRegistryError {}

/// Route a requested model string to the CLI that should run it.
pub fn route_model(model: &str) -> Result<AgentCli, CliRegistryError> {
    let m = model.trim().to_ascii_lowercase();
    if m.is_empty() {
        return Err(CliRegistryError::UnroutableModel(model.to_string()));
    }
    if m.starts_with("jekko:") {
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
    /// CLI driven by this adapter.
    fn cli(&self) -> AgentCli;

    /// Build a headless launch plan.
    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan;
}

/// Return the adapter for a CLI.
#[must_use]
pub fn adapter_for(cli: AgentCli) -> &'static dyn CliAdapter {
    match cli {
        AgentCli::Claude => &ClaudeAdapter,
        AgentCli::Codex => &CodexAdapter,
        AgentCli::Jekko => &JekkoAdapter,
    }
}

/// Adapter for the Claude Code CLI.
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
            args.push("--verbose".to_string());
        }
        if !model.model.is_empty() {
            args.push("--model".to_string());
            args.push(model.model.clone());
        }
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

/// Adapter for the OpenAI Codex CLI.
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAdapter;

impl CliAdapter for CodexAdapter {
    fn cli(&self) -> AgentCli {
        AgentCli::Codex
    }

    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan {
        let mut args = vec!["exec".to_string()];
        if !model.model.is_empty() {
            args.push("-m".to_string());
            args.push(model.model.clone());
        }
        if let Some(effort) = model.effort {
            args.push("-c".to_string());
            args.push(format!("model_reasoning_effort=\"{}\"", effort.as_str()));
        }
        args.push("-s".to_string());
        args.push("workspace-write".to_string());
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

/// Adapter for the Jekko CLI.
#[derive(Debug, Clone, Copy, Default)]
pub struct JekkoAdapter;

impl CliAdapter for JekkoAdapter {
    fn cli(&self) -> AgentCli {
        AgentCli::Jekko
    }

    fn build_launch(&self, program: &str, model: &ModelSelect, stream_json: bool) -> LaunchPlan {
        let mut args = vec![
            "run".to_string(),
            "--headless".to_string(),
            "--ephemeral".to_string(),
        ];
        if let Some(provider) = &model.provider {
            args.push("--provider".to_string());
            args.push(provider.clone());
        }
        if !model.model.is_empty() {
            args.push("--model".to_string());
            args.push(model.model.clone());
        }
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

/// Route a model and build its headless launch plan.
pub fn plan_launch(
    program: &str,
    model: &ModelSelect,
    stream_json: bool,
) -> Result<(AgentCli, LaunchPlan), CliRegistryError> {
    let cli = route_model(&model.model)?;
    let plan = adapter_for(cli).build_launch(program, model, stream_json);
    Ok((cli, plan))
}
