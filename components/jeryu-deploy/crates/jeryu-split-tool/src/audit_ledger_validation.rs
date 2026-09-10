//! Bound validation context; does not authenticate source, policy, or executor claims.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    schema_version: String,
    pub(super) auditor_version: String,
    pub(super) policy_path: String,
    pub(super) minimum: u8,
    pub(super) max_soft: Option<usize>,
    candidate_policy_sha256: String,
    executor_inputs: JsonObject<serde_json::Map<String, Value>>,
}

pub(super) fn context(
    identity: &scheduler::ExecutionIdentity,
    config: &[u8],
    governing: &[u8],
    candidate: &[u8],
) -> Result<Config> {
    ensure!(
        audit_evidence::hash(config) == identity.execution_config_sha256,
        "wrong execution configuration bytes"
    );
    ensure!(
        audit_evidence::hash(governing) == identity.governing_policy_sha256,
        "wrong governing policy bytes"
    );
    let JsonObject(config): JsonObject<Config> = serde_json::from_slice(config)?;
    ensure!(
        config.schema_version == "jeryu.audit-ledger-execution/v1",
        "unsupported ledger execution config"
    );
    ensure!(
        !config.executor_inputs.0.is_empty(),
        "explicit executor input selection required"
    );
    ensure!(
        !config.auditor_version.is_empty()
            && config.auditor_version.len() <= 100
            && config
                .auditor_version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte)),
        "invalid auditor version"
    );
    ensure!(
        config.policy_path == "./agent/audit-policy.toml",
        "ledger requires the current full-audit executor policy path"
    );
    ensure!(
        audit_evidence::hash(candidate) == config.candidate_policy_sha256,
        "wrong candidate policy bytes"
    );
    // A reviewed migration adapter may later prove stronger candidate changes.
    // This first local ledger contract refuses all policy byte changes.
    ensure!(
        governing == candidate,
        "candidate policy migration requires governed comparison; raw policies must currently match"
    );
    let repository = identity
        .repository
        .rsplit('/')
        .next()
        .context("repository owner")?;
    let owner = match identity.scope.as_str() {
        "repository" | "standalone" | "dependency" | "optional" => repository,
        scope if identity.repository == "neverhuman/jeryu" && scope.starts_with("components/") => {
            let owner = scope
                .strip_prefix("components/")
                .context("component scope")?;
            ensure!(
                !owner.is_empty() && !owner.contains('/'),
                "unsupported component scope"
            );
            owner
        }
        _ => bail!("local ledger v1 needs a full repository or explicit components/name scope"),
    };
    let floor = if matches!(owner, "jeryu-cache" | "jeryu-jira" | "jeryu-ci-runner") {
        91
    } else {
        85
    };
    let policy_text = std::str::from_utf8(governing)?;
    ensure!(
        config.minimum == audit_evidence::policy(policy_text, owner, floor)?,
        "execution config lowers or contradicts effective policy floor"
    );
    let policy: toml::Value = toml::from_str(policy_text)?;
    if let Some(tool) = policy.get("required_tool") {
        ensure!(tool.as_str() == Some("jankurai"), "wrong required auditor");
    }
    if let Some(version) = policy.get("required_tool_version") {
        ensure!(
            version.as_str() == Some(config.auditor_version.as_str()),
            "wrong policy auditor version"
        );
    }
    if owner == "jeryu" {
        ensure!(
            config.max_soft == Some(0),
            "root compliance forbids soft findings"
        );
    }
    if let Some(maximum) = policy.get("soft_findings_allowed") {
        let maximum = maximum
            .as_integer()
            .filter(|value| *value >= 0)
            .context("invalid soft finding limit")?;
        ensure!(
            config
                .max_soft
                .is_some_and(|value| value as u128 <= maximum as u128),
            "soft finding limit cannot be relaxed"
        );
    }
    Ok(config)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) schema_version: String,
    pub(super) attempt_id: String,
    pub(super) deduplication_key: String,
    pub(super) source_commit: String,
    pub(super) source_tree: String,
    pub(super) identity: JsonObject<scheduler::ExecutionIdentity>,
    /// Supplied by a local executor adapter; JSON alone cannot authenticate it.
    pub(super) command_exit: Option<i32>,
    pub(super) outcome: Outcome,
    pub(super) reason: String,
    pub(super) report_sha256: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Outcome {
    Report,
    ToolError,
    TimedOut,
    SourceUnavailable,
    Canceled,
}

impl Outcome {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Report => "completed_unqualified",
            Self::ToolError => "tool_error",
            Self::TimedOut => "timed_out",
            Self::SourceUnavailable => "source_unavailable",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Closure {
    pub(super) schema_version: String,
    pub(super) attempt_id: String,
    pub(super) deduplication_key: String,
    pub(super) executor_closed: bool,
    pub(super) reason: String,
}
