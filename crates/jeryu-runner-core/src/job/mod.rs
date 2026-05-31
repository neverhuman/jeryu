#![doc = "Job request parsing and validation."]

mod parse;
mod policy;

#[cfg(test)]
mod tests;

pub use policy::{NetworkPolicy, SecretPolicy, TokenPolicy};

use crate::error::{RunnerError, RunnerResult};
use crate::trust::{RunnerClass, TrustTier};
use std::collections::BTreeMap;
use std::path::PathBuf;

use parse::{
    parse_bool, parse_u64, required, split_csv, validate_env_name, validate_workspace_path,
};

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
