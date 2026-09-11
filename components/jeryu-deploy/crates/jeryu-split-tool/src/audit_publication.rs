//! Private preparation only. No caller-supplied observation grants publication authority.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{audit_census, audit_evidence, audit_score::JsonObject};

#[path = "audit_publication_identity.rs"]
mod identity;
#[path = "audit_publication_storage.rs"]
mod storage;
pub(super) use identity::{ExpectedObservation, Identity, RendererObservation};
pub(super) use storage::create_once;

const MAX_ARTIFACT: usize = 16 * 1024 * 1024;
const MAX_SVG: usize = 1024 * 1024;

pub(super) struct Inputs<'a> {
    pub receipt: &'a [u8],
    pub report: Option<&'a [u8]>,
    pub candidate_policy: &'a [u8],
    pub governing_policy: &'a [u8],
    pub dependency_lock: &'a [u8],
    pub execution_config: &'a [u8],
    pub auditor_receipt: &'a [u8],
    /// Opaque renderer bytes; never exposed as a displayable SVG by this layer.
    pub renderer_output: Option<&'a [u8]>,
    /// Requested artifact could not be admitted as a bounded regular file.
    pub renderer_unavailable: bool,
}

pub(super) struct PreparedBundle {
    source_root: PathBuf,
    destination: String,
    files: BTreeMap<String, Vec<u8>>,
    metadata: Value,
}

impl PreparedBundle {
    pub(super) fn metadata(&self) -> &Value {
        &self.metadata
    }
    pub(super) fn destination(&self) -> &str {
        &self.destination
    }
    pub(super) fn require_publication_admission(&self) -> Result<()> {
        anyhow::bail!(
            "publication blocked: protected execution, policy, sanitization, renderer and publisher admission remain unavailable"
        )
    }
}

fn canonical(value: Value) -> Result<Vec<u8>> {
    Ok(format!("{}\n", crate::canonical_json::pretty(value)?).into_bytes())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: String,
    identity: JsonObject<Identity>,
    command_exit: Option<i32>,
    report_sha256: Option<String>,
}

fn validate_report(
    root: &Path,
    expected: &ExpectedObservation,
    inputs: &Inputs<'_>,
) -> Result<audit_evidence::Summary> {
    identity::validate_inputs(root, expected, inputs)?;
    let JsonObject(receipt): JsonObject<Receipt> = serde_json::from_slice(inputs.receipt)?;
    ensure!(
        receipt.schema_version == "jeryu.audit-publication-observation/v1"
            && receipt.identity.0 == expected.identity
            && receipt.command_exit == expected.command_exit
            && receipt.report_sha256 == expected.report_sha256,
        "observation disagrees with expected source, policy, auditor, workflow or actual command"
    );
    let report = inputs.report.context("full report missing")?;
    ensure!(
        expected.report_sha256.as_deref() == Some(audit_evidence::hash(report).as_str()),
        "wrong report bytes"
    );
    let exit = expected
        .command_exit
        .context("actual command exit unavailable")?;
    ensure!(
        matches!(exit, 0 | 1),
        "auditor command did not complete normally"
    );
    audit_evidence::admit(
        report,
        audit_evidence::Binding {
            commit: &expected.identity.source_commit,
            version: &expected.identity.auditor_version,
            policy_path: "./agent/audit-policy.toml",
            minimum: expected.identity.minimum,
            max_soft: expected.identity.max_soft,
        },
        exit,
    )
}

/// `expected` is a Rust context, never a receipt's trust flag. A future approved
/// workflow must authenticate it independently. This preparer grants no authority.
pub(super) fn prepare(
    root: &Path,
    expected: &ExpectedObservation,
    inputs: Inputs<'_>,
) -> Result<PreparedBundle> {
    expected.validate()?;
    ensure!(
        [
            inputs.receipt,
            inputs.candidate_policy,
            inputs.governing_policy,
            inputs.dependency_lock,
            inputs.execution_config,
            inputs.auditor_receipt
        ]
        .into_iter()
        .chain(inputs.report)
        .all(|bytes| bytes.len() <= MAX_ARTIFACT),
        "preparation input exceeds retention bound"
    );
    ensure!(
        inputs
            .renderer_output
            .is_none_or(|bytes| bytes.len() <= MAX_SVG),
        "renderer output exceeds retention bound"
    );
    let (summary, report_rejection) = match validate_report(root, expected, &inputs) {
        Ok(summary) => (Some(summary), None),
        // Diagnostic data remains private with the raw inputs; it is not a
        // sanitized finding. Bound even parser-produced error descriptions.
        Err(error) => (
            None,
            Some(error.to_string().chars().take(512).collect::<String>()),
        ),
    };
    let mut state = match &summary {
        Some(summary) if summary.passed => "PENDING",
        Some(_) => "FAIL",
        None => "ERROR",
    };
    let mut errors = Vec::new();
    if summary.is_none() {
        errors.push("missing_or_rejected_audit_evidence");
    }
    let score = summary
        .as_ref()
        .filter(|summary| !summary.passed)
        .map(|summary| summary.score);
    let counts = summary
        .as_ref()
        .filter(|summary| !summary.passed)
        .map(|summary| json!({"hard":summary.hard_findings,"caps":summary.caps}));
    let display = json!({"schema_version":"jeryu.audit-display-request/v1",
        "repository":expected.identity.repository,"source_repository":expected.identity.source_repository,"scope":expected.identity.scope,
        "source_commit":expected.identity.source_commit,"source_tree":expected.identity.source_tree,
        "observed_at_unix":expected.identity.observed_at_unix,"state":state,"score":score,
        "minimum":expected.identity.minimum,"counts":counts,"report_relative_path":"report.md",
        "execution_verified":false,"publication_qualified":false});
    let display_bytes = canonical(display)?;
    if inputs.renderer_unavailable {
        errors.push("required_renderer_file_unavailable");
    }
    let renderer_identity_matched = match (&expected.renderer, inputs.renderer_output) {
        (None, None) => false,
        (Some(renderer), Some(svg)) => {
            let matched = renderer.command_exit == 0
                && renderer.source_commit == expected.identity.auditor_source_commit
                && renderer.executable_sha256 == expected.identity.auditor_executable_sha256
                && renderer.receipt_sha256 == expected.identity.auditor_receipt_sha256
                && renderer.display_input_sha256 == audit_evidence::hash(&display_bytes)
                && renderer.svg_sha256 == audit_evidence::hash(svg);
            if !matched {
                errors.push("required_renderer_failed_or_wrong_identity");
            }
            matched
        }
        _ => {
            errors.push("required_renderer_output_or_observation_missing");
            false
        }
    };
    if errors
        .iter()
        .any(|error| error.starts_with("required_renderer_"))
    {
        state = "ERROR";
    }
    let retained_hashes = json!({"receipt":audit_evidence::hash(inputs.receipt),
        "report":inputs.report.map(audit_evidence::hash),
        "renderer_output":inputs.renderer_output.map(audit_evidence::hash),
        "candidate_policy":audit_evidence::hash(inputs.candidate_policy),
        "governing_policy":audit_evidence::hash(inputs.governing_policy),
        "dependency_lock":audit_evidence::hash(inputs.dependency_lock),
        "execution_config":audit_evidence::hash(inputs.execution_config),
        "auditor_receipt":audit_evidence::hash(inputs.auditor_receipt)});
    // The expected immutable attempt identity selects the address. A later
    // different payload for that identity must conflict, never replace bytes.
    let id = audit_evidence::hash(&canonical(serde_json::to_value(expected)?)?);
    let destination = format!(
        "{}/{}/{}/{}/{id}",
        expected.identity.source_repository,
        expected.identity.scope.label(),
        expected
            .identity
            .repository
            .split('/')
            .nth(1)
            .context("logical owner")?,
        expected.identity.source_commit
    );
    let metadata = json!({"schema_version":"jeryu.audit-publication-preparation/v1",
        "destination":destination,"expected_observation":expected,"actual_artifact_hashes":retained_hashes,
        "display_state":state,"display_score":if state=="FAIL" {score} else {None},
        "display_counts":if state=="FAIL" {counts} else {None},
        "renderer_input_is_attempt_request":true,"renderer_file_unavailable":inputs.renderer_unavailable,
        "report_observation":{"qualified":false,"summary":summary,"rejection":report_rejection},"validation_errors":errors,
        "renderer_identity_matched":renderer_identity_matched,"svg_safety_verified":false,
        "source_verified":false,"execution_verified":false,"governing_policy_authenticated":false,
        "renderer_qualified":false,"sanitization_verified":false,"publication_qualified":false,
        "publication_blockers":["authenticated_execution_unavailable","protected_policy_admission_unavailable",
            "qualified_first_party_renderer_unavailable","sanitized_report_admission_unavailable","trusted_publisher_unavailable"]});
    let markdown = format!(
        "{} / {}\n\nPreparation: **{state}**. Publication remains blocked.\n\nSource: `{}` at `{}`\n\nThis is private, unqualified observation data. No verified passing score or live SVG is supplied.\n",
        expected.identity.repository,
        expected.identity.scope.label(),
        expected.identity.source_repository,
        expected.identity.source_commit
    );
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::from([
        ("provenance.json".into(), canonical(metadata.clone())?),
        ("report.md".into(), markdown.into_bytes()),
        ("renderer-input.json".into(), display_bytes),
        ("executor-observation.json".into(), inputs.receipt.to_vec()),
        (
            "candidate-policy.toml".into(),
            inputs.candidate_policy.to_vec(),
        ),
        (
            "governing-policy.toml".into(),
            inputs.governing_policy.to_vec(),
        ),
        (
            "dependency-lock.bin".into(),
            inputs.dependency_lock.to_vec(),
        ),
        (
            "execution-config.json".into(),
            inputs.execution_config.to_vec(),
        ),
        (
            "auditor-receipt.json".into(),
            inputs.auditor_receipt.to_vec(),
        ),
    ]);
    if let Some(report) = inputs.report {
        files.insert("raw-report.json".into(), report.to_vec());
    }
    if let Some(svg) = inputs.renderer_output {
        files.insert("renderer-output.bin".into(), svg.to_vec());
    }
    let manifest: BTreeMap<_, _> = files
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                json!({"sha256":audit_evidence::hash(bytes),"bytes":bytes.len()}),
            )
        })
        .collect();
    files.insert(
        "bundle-manifest.json".into(),
        canonical(json!({"schema_version":"jeryu.audit-private-bundle/v1","files":manifest}))?,
    );
    Ok(PreparedBundle {
        source_root: root.canonicalize()?,
        destination,
        files,
        metadata,
    })
}

#[path = "audit_publication_cli.rs"]
mod cli;
pub(super) use cli::{Arguments, run};

#[cfg(test)]
#[path = "audit_publication_tests.rs"]
mod tests;
