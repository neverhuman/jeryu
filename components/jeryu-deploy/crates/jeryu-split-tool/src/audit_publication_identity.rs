//! Identity matching is separate from future workflow authentication.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Scope {
    Repository,
    Component {
        path: String,
        tree: String,
    },
    Standalone {
        originating_monorepo_commit: String,
        export_provenance_sha256: String,
    },
    Dependency,
    Optional,
}

impl Scope {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Component { .. } => "component",
            Self::Standalone { .. } => "standalone",
            Self::Dependency => "dependency",
            Self::Optional => "optional",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    /// Logical display and policy owner, including a component's downstream slug.
    pub repository: String,
    /// Actual Git repository containing source_commit; never inferred from a card label.
    pub source_repository: String,
    pub scope: Scope,
    pub source_commit: String,
    pub source_tree: String,
    pub auditor_source_commit: String,
    pub auditor_executable_sha256: String,
    pub auditor_receipt_sha256: String,
    pub auditor_version: String,
    pub candidate_policy_sha256: String,
    pub governing_policy_sha256: String,
    pub dependency_lock_sha256: String,
    pub execution_config_sha256: String,
    pub minimum: u8,
    pub max_soft: Option<usize>,
    pub workflow_repository: String,
    pub workflow_path: String,
    pub workflow_commit: String,
    pub workflow_blob: String,
    pub run_id: u64,
    pub run_attempt: u32,
    pub job_id: u64,
    pub observed_at_unix: u64,
}

/// Constructed by Rust callers. No deserializer or boolean can authenticate it.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExpectedObservation {
    pub identity: Identity,
    pub command_exit: Option<i32>,
    pub report_sha256: Option<String>,
    pub renderer: Option<RendererObservation>,
}

/// Independent command observation, never an artifact-supplied trust receipt.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RendererObservation {
    pub source_commit: String,
    pub executable_sha256: String,
    pub receipt_sha256: String,
    pub display_input_sha256: String,
    pub svg_sha256: String,
    pub command_exit: i32,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && value.bytes().any(|byte| byte != b'0')
}
fn slug(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts[0] == "neverhuman"
        && parts[1].len() <= 100
        && parts[1]
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

impl ExpectedObservation {
    pub(super) fn validate(&self) -> Result<()> {
        let i = &self.identity;
        ensure!(
            slug(&i.repository) && slug(&i.source_repository) && slug(&i.workflow_repository),
            "unsupported repository identity"
        );
        ensure!(
            [
                &i.source_commit,
                &i.source_tree,
                &i.auditor_source_commit,
                &i.workflow_commit,
                &i.workflow_blob
            ]
            .into_iter()
            .all(|value| hex(value, 40)),
            "full source/tree/workflow identities required"
        );
        ensure!(
            [
                &i.auditor_executable_sha256,
                &i.auditor_receipt_sha256,
                &i.candidate_policy_sha256,
                &i.governing_policy_sha256,
                &i.dependency_lock_sha256,
                &i.execution_config_sha256
            ]
            .into_iter()
            .all(|value| hex(value, 64))
                && self
                    .report_sha256
                    .as_deref()
                    .is_none_or(|hash| hex(hash, 64)),
            "exact artifact digests required"
        );
        ensure!(
            (85..=100).contains(&i.minimum)
                && i.run_id > 0
                && i.run_attempt > 0
                && i.job_id > 0
                && i.observed_at_unix > 0,
            "invalid policy or workflow attempt identity"
        );
        ensure!(
            !i.auditor_version.is_empty()
                && i.auditor_version.len() <= 100
                && i.auditor_version
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b".-+".contains(&byte)),
            "invalid auditor version"
        );
        ensure!(
            i.workflow_path.starts_with(".github/workflows/")
                && i.workflow_path.len() <= 512
                && i.workflow_path.split('/').all(|part| !part.is_empty()
                    && !matches!(part, "." | "..")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))),
            "invalid workflow path"
        );
        match &i.scope {
            Scope::Component { path, tree } => ensure!(
                i.source_repository == "neverhuman/jeryu"
                    && i.repository != "neverhuman/jeryu"
                    && path == &format!("components/{}", i.repository.split('/').nth(1).unwrap())
                    && hex(tree, 40),
                "component identity mismatch"
            ),
            Scope::Standalone {
                originating_monorepo_commit,
                export_provenance_sha256,
            } => ensure!(
                i.source_repository == i.repository
                    && i.repository != "neverhuman/jeryu"
                    && hex(originating_monorepo_commit, 40)
                    && hex(export_provenance_sha256, 64),
                "standalone origin identity missing or source repository differs"
            ),
            _ => ensure!(
                i.source_repository == i.repository,
                "repository source differs from logical audit owner"
            ),
        }
        if let Some(renderer) = &self.renderer {
            ensure!(
                hex(&renderer.source_commit, 40)
                    && [
                        &renderer.executable_sha256,
                        &renderer.receipt_sha256,
                        &renderer.display_input_sha256,
                        &renderer.svg_sha256
                    ]
                    .into_iter()
                    .all(|value| hex(value, 64)),
                "invalid renderer observation identity"
            );
        }
        Ok(())
    }
}

pub(super) fn validate_inputs(
    root: &Path,
    expected: &ExpectedObservation,
    inputs: &Inputs<'_>,
) -> Result<()> {
    let i = &expected.identity;
    let pin_bytes = storage::read_file(
        &root.join("components/jeryu-tool/generated/jankurai-pin.env"),
        MAX_ARTIFACT,
    )?;
    let pin_source = std::str::from_utf8(&pin_bytes)?;
    ensure!(
        audit_census::publication_pin(pin_source, "JANKURAI_REV")? == i.auditor_source_commit
            && audit_census::publication_pin(pin_source, "JANKURAI_BINARY_SHA256")?
                == i.auditor_executable_sha256
            && audit_census::publication_pin(pin_source, "JANKURAI_SEMVER")? == i.auditor_version,
        "auditor identity differs from owning projection"
    );
    for (bytes, hash) in [
        (inputs.candidate_policy, &i.candidate_policy_sha256),
        (inputs.governing_policy, &i.governing_policy_sha256),
        (inputs.dependency_lock, &i.dependency_lock_sha256),
        (inputs.execution_config, &i.execution_config_sha256),
        (inputs.auditor_receipt, &i.auditor_receipt_sha256),
    ] {
        ensure!(
            audit_evidence::hash(bytes) == *hash,
            "bound validation input differs"
        );
    }
    ensure!(
        inputs.candidate_policy == inputs.governing_policy,
        "reviewed candidate-policy comparison unavailable"
    );
    let owner = i.repository.split('/').nth(1).context("policy owner")?;
    ensure!(
        audit_evidence::policy(
            std::str::from_utf8(inputs.governing_policy)?,
            owner,
            audit_census::effective_floor(owner)
        )? == i.minimum,
        "effective policy floor differs"
    );
    let policy: toml::Value = toml::from_str(std::str::from_utf8(inputs.governing_policy)?)?;
    if let Some(tool) = policy.get("required_tool") {
        ensure!(tool.as_str() == Some("jankurai"), "wrong required auditor");
    }
    if let Some(version) = policy.get("required_tool_version") {
        ensure!(
            version.as_str() == Some(i.auditor_version.as_str()),
            "wrong required auditor version"
        );
    }
    if owner == "jeryu" && matches!(i.scope, Scope::Repository) {
        ensure!(
            i.max_soft == Some(0),
            "root soft finding policy must remain strict"
        );
    }
    if let Some(limit) = policy.get("soft_findings_allowed") {
        let limit = limit
            .as_integer()
            .filter(|value| *value >= 0)
            .context("invalid soft limit")?;
        ensure!(
            i.max_soft
                .is_some_and(|value| value as u128 <= limit as u128),
            "soft finding limit was relaxed"
        );
    }
    Ok(())
}
