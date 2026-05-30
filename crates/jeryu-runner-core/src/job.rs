#![doc = "Job request parsing and validation."]

use crate::error::{RunnerError, RunnerResult};
use crate::fscheck::deny_dangerous_host_path;
use crate::trust::{RunnerClass, TrustTier};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Network policy requested for the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// No network namespace connectivity.
    Deny,
    /// Loopback-only access.
    LoopbackOnly,
    /// Egress-only network access.
    EgressOnly,
}

impl NetworkPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::LoopbackOnly => "loopback-only",
            Self::EgressOnly => "egress-only",
        }
    }
}

impl std::str::FromStr for NetworkPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "deny" | "none" | "off" => Ok(Self::Deny),
            "loopback" | "loopback-only" => Ok(Self::LoopbackOnly),
            "egress" | "egress-only" => Ok(Self::EgressOnly),
            _ => Err(RunnerError::new(
                "invalid_network_policy",
                format!("unknown network policy '{value}'"),
            )),
        }
    }
}

/// Secret policy requested by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPolicy {
    /// No secrets are provided.
    None,
    /// Use default tier-based secret policy.
    Default,
    /// Explicitly request secrets.
    Requested,
}

impl SecretPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Default => "default",
            Self::Requested => "requested",
        }
    }
}

impl std::str::FromStr for SecretPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" | "deny" | "false" => Ok(Self::None),
            "default" | "auto" => Ok(Self::Default),
            "requested" | "request" | "true" => Ok(Self::Requested),
            _ => Err(RunnerError::new(
                "invalid_secret_policy",
                format!("unknown secret policy '{value}'"),
            )),
        }
    }
}

/// Token policy for step-scoped credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPolicy {
    /// No token material.
    None,
    /// Read-only token.
    ReadOnly,
    /// Scoped write token.
    ScopedWrite,
}

impl TokenPolicy {
    /// Stable policy label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ReadOnly => "read-only",
            Self::ScopedWrite => "scoped-write",
        }
    }
}

impl std::str::FromStr for TokenPolicy {
    type Err = RunnerError;

    fn from_str(value: &str) -> RunnerResult<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "none" | "deny" => Ok(Self::None),
            "read" | "read-only" | "readonly" => Ok(Self::ReadOnly),
            "write" | "scoped-write" => Ok(Self::ScopedWrite),
            _ => Err(RunnerError::new(
                "invalid_token_policy",
                format!("unknown token policy '{value}'"),
            )),
        }
    }
}

/// Immutable job request consumed by jeryu_runnerd.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRequest {
    /// Stable job id.
    pub job_id: String,
    /// Repository id or slug.
    pub repo_id: String,
    /// Commit SHA bound to this job.
    pub commit_sha: String,
    /// Worktree directory.
    pub workspace: PathBuf,
    /// Command executable.
    pub command: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// Environment allowlist requested by the job.
    pub env: BTreeMap<String, String>,
    /// Trust tier.
    pub trust_tier: TrustTier,
    /// Optional requested runner class.
    pub requested_runner: Option<RunnerClass>,
    /// Requested network policy.
    pub network_policy: NetworkPolicy,
    /// Secret policy.
    pub secret_policy: SecretPolicy,
    /// Step token policy.
    pub token_policy: TokenPolicy,
    /// Timeout in milliseconds.
    pub timeout_ms: u64,
    /// True when the job is from a fork.
    pub fork: bool,
}

impl JobRequest {
    /// Parse a simple key-value job file.
    ///
    /// Supported syntax:
    ///
    /// ```text
    /// job_id=job_1
    /// repo_id=org/repo
    /// commit_sha=abc123
    /// workspace=/tmp/work
    /// command=/bin/echo
    /// args=hello,world
    /// trust_tier=T1
    /// requested_runner=native-rust-hot
    /// env.RUST_LOG=info
    /// ```
    pub fn from_key_value(input: &str) -> RunnerResult<Self> {
        let mut values = BTreeMap::<String, String>::new();
        let mut env = BTreeMap::<String, String>::new();

        for (line_no, raw_line) in input.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(RunnerError::new(
                    "invalid_job_file",
                    format!("line {} is not key=value", line_no + 1),
                ));
            };
            let key = key.trim();
            let value = value.trim();
            if let Some(name) = key.strip_prefix("env.") {
                validate_env_name(name)?;
                env.insert(name.to_string(), value.to_string());
            } else {
                values.insert(key.to_string(), value.to_string());
            }
        }

        let job_id = required(&values, "job_id")?;
        let repo_id = required(&values, "repo_id")?;
        let commit_sha = required(&values, "commit_sha")?;
        let workspace = PathBuf::from(required(&values, "workspace")?);
        let command = required(&values, "command")?;
        let args = values
            .get("args")
            .map(|raw| split_csv(raw))
            .unwrap_or_default();
        let trust_tier = values
            .get("trust_tier")
            .map(|raw| raw.parse())
            .transpose()?
            .unwrap_or(TrustTier::T5PublicUntrusted);
        let requested_runner = values
            .get("requested_runner")
            .map(|raw| raw.parse())
            .transpose()?;
        let network_policy = values
            .get("network_policy")
            .map(|raw| raw.parse())
            .transpose()?
            .unwrap_or(NetworkPolicy::Deny);
        let secret_policy = values
            .get("secret_policy")
            .map(|raw| raw.parse())
            .transpose()?
            .unwrap_or(SecretPolicy::Default);
        let token_policy = values
            .get("token_policy")
            .map(|raw| raw.parse())
            .transpose()?
            .unwrap_or(TokenPolicy::ReadOnly);
        let timeout_ms = values
            .get("timeout_ms")
            .map(|raw| parse_u64("timeout_ms", raw))
            .transpose()?
            .unwrap_or(600_000);
        let fork = values
            .get("fork")
            .map(|raw| parse_bool("fork", raw))
            .transpose()?
            .unwrap_or(matches!(trust_tier, TrustTier::T4ForkPr));

        let request = Self {
            job_id,
            repo_id,
            commit_sha,
            workspace,
            command,
            args,
            env,
            trust_tier,
            requested_runner,
            network_policy,
            secret_policy,
            token_policy,
            timeout_ms,
            fork,
        };
        request.validate()?;
        Ok(request)
    }

    /// Validate request invariants that are independent of host state.
    pub fn validate(&self) -> RunnerResult<()> {
        if self.job_id.trim().is_empty() {
            return Err(RunnerError::new("invalid_job", "job_id cannot be empty"));
        }
        if self.repo_id.trim().is_empty() {
            return Err(RunnerError::new("invalid_job", "repo_id cannot be empty"));
        }
        if self.commit_sha.trim().is_empty() {
            return Err(RunnerError::new(
                "invalid_job",
                "commit_sha cannot be empty",
            ));
        }
        if self.command.trim().is_empty() {
            return Err(RunnerError::new("invalid_job", "command cannot be empty"));
        }
        validate_workspace_path(&self.workspace)?;
        if self.timeout_ms == 0 {
            return Err(RunnerError::new(
                "invalid_job",
                "timeout_ms must be positive",
            ));
        }
        Ok(())
    }
}

fn required(values: &BTreeMap<String, String>, key: &'static str) -> RunnerResult<String> {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| RunnerError::new("missing_job_field", format!("missing {key}")))
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_u64(field: &'static str, value: &str) -> RunnerResult<u64> {
    value.trim().parse::<u64>().map_err(|err| {
        RunnerError::new(
            "invalid_integer",
            format!("{field} must be an unsigned integer: {err}"),
        )
    })
}

fn parse_bool(field: &'static str, value: &str) -> RunnerResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(RunnerError::new(
            "invalid_bool",
            format!("{field} must be true/false"),
        )),
    }
}

fn validate_env_name(name: &str) -> RunnerResult<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && !name.chars().next().is_some_and(|ch| ch.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(RunnerError::new(
            "invalid_env_name",
            format!("invalid environment variable '{name}'"),
        ))
    }
}

fn validate_workspace_path(path: &Path) -> RunnerResult<()> {
    if !path.is_absolute() {
        return Err(RunnerError::new(
            "invalid_workspace",
            format!("workspace '{}' must be absolute", path.display()),
        ));
    }
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(RunnerError::new(
            "invalid_workspace",
            format!("workspace '{}' must not contain '..'", path.display()),
        ));
    }
    deny_dangerous_host_path(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_job() {
        let job = JobRequest::from_key_value(
            r#"
            job_id=job_1
            repo_id=jeryu/jeryu
            commit_sha=abc123
            workspace=/tmp/jeryu-work
            command=/bin/echo
            args=hello,world
            trust_tier=T1
            requested_runner=native-rust-hot
            env.RUST_LOG=info
            "#,
        )
        .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(job.args, vec!["hello", "world"]);
        assert_eq!(job.trust_tier, TrustTier::T1ProtectedInternal);
        assert_eq!(job.requested_runner, Some(RunnerClass::NativeRustHot));
    }

    #[test]
    fn rejects_relative_workspace() {
        let err = JobRequest::from_key_value(
            r#"
            job_id=job_1
            repo_id=jeryu/jeryu
            commit_sha=abc123
            workspace=relative
            command=/bin/echo
            "#,
        )
        .err()
        .unwrap_or_else(|| panic!("expected validation failure"));
        assert_eq!(err.code(), "invalid_workspace");
    }

    #[test]
    fn rejects_dangerous_workspace_paths() {
        let err = JobRequest::from_key_value(
            r#"
            job_id=job_1
            repo_id=jeryu/jeryu
            commit_sha=abc123
            workspace=/var/run/docker.sock
            command=/bin/echo
            "#,
        )
        .err()
        .unwrap_or_else(|| panic!("expected host path denial"));
        assert_eq!(err.code(), "host_path_denied");
        assert!(err.message().contains("/var/run/docker.sock"));
    }

    #[test]
    fn rejects_dangerous_workspace_path_children() {
        let err = JobRequest::from_key_value(
            r#"
            job_id=job_1
            repo_id=jeryu/jeryu
            commit_sha=abc123
            workspace=/root/.ssh/id_rsa
            command=/bin/echo
            "#,
        )
        .err()
        .unwrap_or_else(|| panic!("expected host path denial"));
        assert_eq!(err.code(), "host_path_denied");
        assert!(err.message().contains("/root/.ssh/id_rsa"));
    }
}
