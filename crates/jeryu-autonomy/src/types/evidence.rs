//! Canonical object 3: the Evidence Pack — the signed, SHA-bound bundle of
//! everything the gate reasons over (changed files, tests, security scans,
//! supply-chain facts, rollback plan, and required CI/proof receipts).

use super::common::RiskTier;
use super::schema_tag::{EvidencePackTag, SchemaTag};
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ChangedFile {
    pub path: String,
    #[serde(default)]
    pub risk_tags: Vec<String>,
    pub lines_added: u32,
    pub lines_removed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanOutcome {
    Passed,
    Failed,
    Skipped,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestsSection {
    #[serde(default)]
    pub targeted: Vec<String>,
    #[serde(default)]
    pub full_required: bool,
    #[serde(default)]
    pub skipped: Vec<String>,
    #[serde(default)]
    pub coverage_delta: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecuritySection {
    pub sast: ScanOutcome,
    pub dependency_scan: ScanOutcome,
    pub secret_scan: ScanOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SupplyChainSection {
    #[serde(default)]
    pub dependency_changes: Vec<serde_json::Value>,
    #[serde(default)]
    pub external_code_sources: Vec<String>,
    #[serde(default)]
    pub lockfile_only_change: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackStrategy {
    RevertCommit,
    FeatureFlag,
    DataMigrationReverse,
    RedeployPrevious,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackSection {
    pub strategy: RollbackStrategy,
    #[serde(default)]
    pub feature_flag: Option<String>,
    #[serde(default)]
    pub data_migration_reversible: Option<bool>,
}

/// Slice carrying a required proof/CI-lane Receipt entry. A passing jeryu-ci
/// lane maps to one of these; the seam [`crate::seam::ProofReceipt`] is the
/// thin stand-in for the forge's Receipt type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateReceipt {
    pub id: String,
    pub status: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidencePack {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<EvidencePackTag>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    pub repo: String,
    pub source_branch: String,
    pub target_branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub policy_sha: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_agent: Option<String>,
    pub risk: RiskTier,
    #[serde(default)]
    pub changed_files: Vec<ChangedFile>,
    #[serde(default)]
    pub claims: Vec<String>,
    pub tests: TestsSection,
    pub security: SecuritySection,
    pub supply_chain: SupplyChainSection,
    pub rollback: RollbackSection,
    #[serde(default)]
    pub gate_receipts: Vec<GateReceipt>,
    pub evidence_digest: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}
