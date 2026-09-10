//! Admit an existing auditor report using its owning repository's score policy.

use std::fmt;
use std::fs;
use std::marker::PhantomData;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum Owner {
    Jeryu,
    JeryuCore,
    JeryuDeploy,
    JeryuJira,
    JeryuIntelligence,
    JeryuReleaseOps,
    JeryuTool,
    JeryuWeb,
}

impl Owner {
    fn policy(self) -> (&'static str, u8) {
        match self {
            Self::Jeryu => ("jeryu", 85),
            Self::JeryuCore => ("jeryu-core", 85),
            Self::JeryuDeploy => ("jeryu-deploy", 85),
            Self::JeryuJira => ("jeryu-jira", 85),
            Self::JeryuIntelligence => ("jeryu-intelligence", 85),
            Self::JeryuReleaseOps => ("jeryu-release-ops", 85),
            Self::JeryuTool => ("jeryu-tool", 85),
            Self::JeryuWeb => ("jeryu-web", 85),
        }
    }
}

// JSON sequences are valid input to ordinary derived structs. Gate objects must
// use maps; retain the inner typed decoder's duplicate-field checks as well.
pub(super) struct JsonObject<T>(pub(super) T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for JsonObject<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = JsonObject<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(JsonObject)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

#[derive(Deserialize)]
struct Policy {
    minimum_score: u8,
    workspace: Option<String>,
}

// Defaults apply only to absent fields. Explicit null values must fail decoding.
// Other auditor fields remain available to their separate owning proof gates.
#[derive(Deserialize)]
struct Report {
    score: u8,
    caps_applied: Vec<Value>,
    #[serde(default)]
    caps: Vec<Value>,
    findings: Vec<JsonObject<Finding>>,
    decision: JsonObject<Decision>,
    #[serde(default)]
    hard_findings: u64,
}

#[derive(Deserialize)]
struct Decision {
    #[serde(default)]
    hard_findings: u64,
}

#[derive(Deserialize)]
struct Finding {
    severity: String,
    #[serde(default = "soft_hardness")]
    hardness: String,
}

fn soft_hardness() -> String {
    "soft".to_owned()
}

#[derive(Debug, PartialEq, Eq)]
struct AcceptedScore {
    score: u8,
    minimum_score: u8,
}

fn check(owner: Owner, policy_source: &str, report_source: &str) -> Result<AcceptedScore> {
    let policy: Policy = toml::from_str(policy_source).context("invalid audit policy")?;
    let (workspace, minimum) = owner.policy();
    ensure!(
        (minimum..=100).contains(&policy.minimum_score),
        "audit policy minimum_score must be an integer from {minimum} to 100"
    );
    if let Some(identity) = policy.workspace {
        ensure!(
            identity == workspace,
            "audit policy workspace does not match owner"
        );
    }

    // Typed decoding rejects noninteger scores/counts, malformed known fields,
    // duplicate gate fields, and anything after the single JSON document.
    let JsonObject(report): JsonObject<Report> =
        serde_json::from_str(report_source).context("invalid audit report")?;
    ensure!(
        report.score <= 100,
        "audit score must be an integer from 0 to 100"
    );
    ensure!(
        report.score >= policy.minimum_score,
        "audit score {} is below {}",
        report.score,
        policy.minimum_score
    );
    ensure!(
        report.caps_applied.is_empty() && report.caps.is_empty(),
        "audit caps present"
    );
    for JsonObject(finding) in &report.findings {
        ensure!(
            matches!(
                finding.severity.as_str(),
                "critical" | "high" | "medium" | "low" | "info"
            ),
            "audit finding has an invalid severity"
        );
        ensure!(
            matches!(finding.hardness.as_str(), "soft" | "hard"),
            "audit finding has an invalid hardness"
        );
    }
    let actual_hard = report.findings.iter().any(|JsonObject(finding)| {
        matches!(finding.severity.as_str(), "critical" | "high") || finding.hardness == "hard"
    });
    ensure!(
        !actual_hard && report.hard_findings == 0 && report.decision.0.hard_findings == 0,
        "audit hard findings present"
    );
    Ok(AcceptedScore {
        score: report.score,
        minimum_score: policy.minimum_score,
    })
}

pub(super) fn run(owner: Owner, policy: &Path, report: &Path) -> Result<()> {
    let policy_source = fs::read_to_string(policy)
        .with_context(|| format!("read audit policy {}", policy.display()))?;
    let report_source = fs::read_to_string(report)
        .with_context(|| format!("read audit report {}", report.display()))?;
    let accepted = check(owner, &policy_source, &report_source)?;
    println!(
        "score ok: score={} floor={}",
        accepted.score, accepted.minimum_score
    );
    Ok(())
}

#[cfg(test)]
#[path = "audit_score_tests.rs"]
mod tests;
