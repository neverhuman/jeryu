//! Report admission for the census. A numeric score alone is never a decision.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::audit_score::JsonObject;

#[derive(Debug, Serialize)]
pub(super) struct Summary {
    pub score: u8,
    pub minimum: u8,
    pub hard_findings: usize,
    pub soft_findings: usize,
    pub caps: usize,
    pub passed: bool,
}

#[derive(Deserialize)]
struct Report {
    standard: String,
    schema_version: String,
    auditor_version: String,
    repo: String,
    score: u8,
    dirty_worktree: bool,
    scope: JsonObject<Scope>,
    git: JsonObject<Git>,
    policy: JsonObject<ReportPolicy>,
    decision: JsonObject<Decision>,
    conformance_decision: String,
    conformance_blockers: Vec<Value>,
    findings: Vec<JsonObject<Finding>>,
    caps_applied: Vec<Value>,
    #[serde(default)]
    caps: Vec<Value>,
    #[serde(default, deserialize_with = "present_count")]
    hard_findings: Option<usize>,
}

fn present_count<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<usize>, D::Error> {
    usize::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct Scope {
    mode: String,
    paths: Vec<String>,
}
#[derive(Deserialize)]
struct Git {
    head: String,
    mode: String,
    dirty_worktree: bool,
}
#[derive(Deserialize)]
struct ReportPolicy {
    minimum_score: u8,
    mode: String,
    path: String,
    fail_on: Vec<String>,
}
#[derive(Deserialize)]
struct Decision {
    status: String,
    passed: bool,
    minimum_score: u8,
    hard_findings: usize,
    soft_findings: usize,
    ratchet: JsonObject<Ratchet>,
}
#[derive(Deserialize)]
struct Ratchet {
    passed: bool,
}
#[derive(Deserialize)]
struct Finding {
    severity: String,
    hardness: String,
}
#[derive(Deserialize)]
struct Policy {
    workspace: String,
    minimum_score: u8,
    hard_findings_allowed: u64,
}

pub(super) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn policy(source: &str, owner: &str, floor: u8) -> Result<u8> {
    let policy: Policy = toml::from_str(source).context("invalid owning audit policy")?;
    ensure!(
        policy.workspace == owner,
        "policy belongs to another repository"
    );
    ensure!(
        policy.minimum_score <= 100 && floor <= 100,
        "invalid policy floor"
    );
    ensure!(
        policy.hard_findings_allowed == 0,
        "policy permits hard findings"
    );
    Ok(policy.minimum_score.max(floor).max(85))
}

pub(super) struct Binding<'a> {
    pub commit: &'a str,
    pub version: &'a str,
    pub policy_path: &'a str,
    pub minimum: u8,
    pub max_soft: Option<usize>,
}

/// Called only for a freshly executed command, with its real exit status.
/// Malformed and contradictory evidence is an error, including exit != 0
/// followed by a report claiming success.
pub(super) fn admit(bytes: &[u8], binding: Binding<'_>, exit: i32) -> Result<Summary> {
    let JsonObject(report): JsonObject<Report> =
        serde_json::from_slice(bytes).context("missing, truncated or invalid full audit report")?;
    ensure!(
        report.standard == "jankurai" && report.schema_version == "1.9.0",
        "unsupported report schema"
    );
    ensure!(
        report.auditor_version == binding.version,
        "wrong auditor version"
    );
    ensure!(report.repo == ".", "wrong audit repository");
    ensure!(
        !report.dirty_worktree && !report.git.0.dirty_worktree,
        "dirty audited source"
    );
    ensure!(
        report.scope.0.mode == "full"
            && report.scope.0.paths.is_empty()
            && report.git.0.mode == "full",
        "partial audit cannot qualify"
    );
    let head = &report.git.0.head;
    ensure!(
        (7..=40).contains(&head.len()) && binding.commit.starts_with(head),
        "wrong report commit"
    );
    ensure!(report.score <= 100, "invalid score");
    ensure!(
        report.policy.0.path == binding.policy_path
            && report.policy.0.minimum_score == binding.minimum
            && report.decision.0.minimum_score == binding.minimum
            && report.policy.0.mode == "standard",
        "wrong report policy"
    );
    let mut hard = 0;
    let mut soft = 0;
    let mut producer_hard = 0;
    ensure!(
        report.policy.0.fail_on.len() == 2
            && report.policy.0.fail_on.contains(&"critical".into())
            && report.policy.0.fail_on.contains(&"high".into()),
        "wrong producer severity policy"
    );
    for JsonObject(finding) in &report.findings {
        ensure!(
            matches!(
                finding.severity.as_str(),
                "critical" | "high" | "medium" | "low" | "info"
            ),
            "invalid finding severity"
        );
        ensure!(
            matches!(finding.hardness.as_str(), "hard" | "soft"),
            "invalid finding hardness"
        );
        if finding.hardness == "hard" || matches!(finding.severity.as_str(), "critical" | "high") {
            hard += 1;
        } else {
            soft += 1;
        }
        if matches!(finding.severity.as_str(), "critical" | "high") {
            producer_hard += 1;
        }
    }
    let decision = report.decision.0;
    ensure!(
        decision.hard_findings == producer_hard
            && decision.soft_findings == report.findings.len() - producer_hard
            && report.hard_findings.is_none_or(|count| count == hard),
        "contradictory finding counts"
    );
    ensure!(
        matches!(decision.status.as_str(), "pass" | "fail")
            && decision.passed == (decision.status == "pass"),
        "contradictory policy decision"
    );
    let caps = report.caps_applied.len() + report.caps.len();
    let score_passes = report.score >= binding.minimum && hard == 0 && caps == 0;
    ensure!(
        !decision.passed || (score_passes && decision.ratchet.0.passed && exit == 0),
        "producer reported success despite failed execution, findings, caps or ratchet"
    );
    let passed = score_passes
        && decision.passed
        && decision.ratchet.0.passed
        && exit == 0
        && report.conformance_decision == "pass"
        && report.conformance_blockers.is_empty()
        && binding.max_soft.is_none_or(|maximum| soft <= maximum);
    Ok(Summary {
        score: report.score,
        minimum: binding.minimum,
        hard_findings: hard,
        soft_findings: soft,
        caps,
        passed,
    })
}

#[cfg(test)]
#[path = "audit_evidence_tests.rs"]
mod tests;
