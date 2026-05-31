//! Wire schema for the autonomy gate (self-contained port of the canonical
//! typed objects this crate operates on).
//!
//! Placement note: in the fused workspace these types are slated to live in
//! `jeryu-proof` (Codex-owned) and be re-exported here. Until that lands this
//! crate hosts them so it builds and tests in isolation. Field shapes are kept
//! byte-stable for receipt/verdict canonical-JSON signing and replay.

mod tag;

pub use tag::{
    AgentApprovalReceiptTag, EvidencePackTag, SchemaKind, SchemaTag, VibeGateVerdictTag,
};

use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Risk tier ladder. Lower tiers are auto-merge-eligible; higher tiers require
/// a human.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RiskTier {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
}

impl RiskTier {
    pub fn auto_merge_eligible(self) -> bool {
        matches!(self, RiskTier::R0 | RiskTier::R1 | RiskTier::R2)
    }
    pub fn human_required(self) -> bool {
        matches!(self, RiskTier::R3 | RiskTier::R4 | RiskTier::R5)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerRole {
    Security,
    TestIntegrity,
    Runtime,
    Lockfile,
    Judge,
    ReleaseShepherd,
    Nightwatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Pass,
    Concern,
    Block,
    Abstain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    AllowMerge,
    RequireHuman,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GateReceipt {
    pub id: String,
    pub status: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// The change-under-review bundle the judge fuses over. Carries the
/// `(id, head_sha, policy_sha)` tuple every receipt must SHA-bind to (Law 4).
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    pub class: String,
    pub file: String,
    pub range: [u32; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TokenCounts {
    pub prompt: u32,
    pub completion: u32,
}

/// One reviewer's signed verdict on a single `EvidencePack`. SHA-bound to the
/// pack's `(evidence_pack_id, head_sha, policy_sha)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentApprovalReceipt {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<AgentApprovalReceiptTag>,
    pub id: String,
    pub evidence_pack_id: String,
    pub role: ReviewerRole,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_response_sha: Option<String>,
    pub head_sha: String,
    pub policy_sha: String,
    pub decision: ReviewDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default = "true_default")]
    pub not_author: bool,
    #[serde(default)]
    pub tokens: TokenCounts,
    pub created_at: DateTime<Utc>,
    pub signature: Signature,
}

fn true_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerdictReceiptRef {
    pub role: ReviewerRole,
    pub agent_id: String,
    pub receipt_digest: String,
    pub decision: ReviewDecision,
    pub not_author: bool,
}

/// The judge's fused verdict over a pack + its bound receipts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeGateVerdict {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<VibeGateVerdictTag>,
    pub id: String,
    pub evidence_pack_id: String,
    /// Pull-request reference (string id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<String>,
    pub repo: String,
    pub target_branch: String,
    pub head_sha: String,
    pub policy_sha: String,
    pub evidence_pack_digest: String,
    pub risk: RiskTier,
    #[serde(default)]
    pub hard_stops: Vec<String>,
    #[serde(default)]
    pub required_reviews: Vec<ReviewerRole>,
    #[serde(default)]
    pub approval_receipts: Vec<VerdictReceiptRef>,
    pub decision: GateDecision,
    pub valid_for_head_sha_only: bool,
    #[serde(default = "true_default")]
    pub rebind_on_train: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub signature: Signature,
}
