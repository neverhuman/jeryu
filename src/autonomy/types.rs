//! The 8 canonical typed objects.
//!
//! Schemas: `.jeryu/autonomy/schemas/*.schema.json`. These Rust types and those JSON
//! schemas evolve together; any change must update BOTH (CI lints this).

use crate::autonomy::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One of the 6 risk tiers from tip1.
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

// ---------------------------------------------------------------------------
// 1. Intent Card
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentCard {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<IntentCardTag>,
    pub id: String,
    pub agent_id: String,
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_risk: Option<RiskTier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_changed_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

// ---------------------------------------------------------------------------
// 2. Capability Lease
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LeaseScope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_write_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityLease {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<CapabilityLeaseTag>,
    pub id: String,
    pub intent_id: String,
    pub agent_id: String,
    pub scope: LeaseScope,
    pub ttl_seconds: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub policy_sha: String,
    pub signature: Signature,
}

impl CapabilityLease {
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Pre-flight check: may this lease perform `action` on `repo` touching
    /// `paths`, run by `agent_id`, at time `now`?
    ///
    /// Refuses on: expired, agent_id mismatch, action not in allowed_actions,
    /// action explicitly in denied_actions, or any path matching denied_paths.
    /// `repo` is opaque here — the lease doesn't carry a repo scope yet; the
    /// orchestrator binds lease → intent_id → repo upstream. Reserved for a
    /// future schema bump that adds `LeaseScope::repos: Vec<String>`.
    pub fn permits(
        &self,
        action: &str,
        agent_id: &str,
        paths: &[&str],
        now: DateTime<Utc>,
    ) -> Result<(), LeaseDenied> {
        if self.is_expired_at(now) {
            return Err(LeaseDenied::Expired {
                expired_at: self.expires_at,
                now,
            });
        }
        if self.agent_id != agent_id {
            return Err(LeaseDenied::AgentIdMismatch {
                lease_agent: self.agent_id.clone(),
                request_agent: agent_id.to_string(),
            });
        }
        if self.scope.denied_actions.iter().any(|a| a == action) {
            return Err(LeaseDenied::ActionDenied(action.to_string()));
        }
        if !self.scope.allowed_actions.is_empty()
            && !self.scope.allowed_actions.iter().any(|a| a == action)
        {
            return Err(LeaseDenied::ActionNotAllowed(action.to_string()));
        }
        for p in paths {
            for denied in &self.scope.denied_paths {
                if path_matches_glob(denied, p) {
                    return Err(LeaseDenied::PathDenied {
                        path: (*p).to_string(),
                        pattern: denied.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseDenied {
    Expired {
        expired_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
    AgentIdMismatch {
        lease_agent: String,
        request_agent: String,
    },
    ActionDenied(String),
    ActionNotAllowed(String),
    PathDenied {
        path: String,
        pattern: String,
    },
}

impl std::fmt::Display for LeaseDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeaseDenied::Expired { expired_at, now } => {
                write!(f, "lease expired at {expired_at}; now {now}")
            }
            LeaseDenied::AgentIdMismatch {
                lease_agent,
                request_agent,
            } => write!(
                f,
                "lease was issued to '{lease_agent}'; request came from '{request_agent}'"
            ),
            LeaseDenied::ActionDenied(a) => write!(f, "action '{a}' explicitly denied by lease"),
            LeaseDenied::ActionNotAllowed(a) => {
                write!(f, "action '{a}' not in lease's allowed_actions allowlist")
            }
            LeaseDenied::PathDenied { path, pattern } => {
                write!(f, "path '{path}' matches denied pattern '{pattern}'")
            }
        }
    }
}

impl std::error::Error for LeaseDenied {}

/// Minimal glob matcher for lease denied_paths: `*` within segment, `**`
/// across segments. Anchored at root (no `/` prefix needed).
fn path_matches_glob(pattern: &str, path: &str) -> bool {
    glob_inner(pattern.as_bytes(), 0, path.as_bytes(), 0)
}

fn glob_inner(p: &[u8], pi: usize, s: &[u8], si: usize) -> bool {
    let mut pi = pi;
    let mut si = si;
    while pi < p.len() {
        if p[pi] == b'*' {
            let double = pi + 1 < p.len() && p[pi + 1] == b'*';
            if double {
                pi += 2;
                if pi < p.len() && p[pi] == b'/' {
                    pi += 1;
                }
                if pi >= p.len() {
                    return true;
                }
                for try_si in si..=s.len() {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            } else {
                pi += 1;
                if pi >= p.len() {
                    return !s[si..].contains(&b'/');
                }
                let limit = s[si..]
                    .iter()
                    .position(|c| *c == b'/')
                    .map(|n| si + n)
                    .unwrap_or(s.len());
                for try_si in si..=limit {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            }
        } else if si < s.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else {
            return false;
        }
    }
    si == s.len()
}

// ---------------------------------------------------------------------------
// 3. Evidence Pack
// ---------------------------------------------------------------------------

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

/// Slice carrying required src/release/gate.rs::Receipt entries.
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

// ---------------------------------------------------------------------------
// 4. Agent Approval Receipt
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// 5. VibeGate Verdict
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerdictReceiptRef {
    pub role: ReviewerRole,
    pub agent_id: String,
    pub receipt_digest: String,
    pub decision: ReviewDecision,
    pub not_author: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeGateVerdict {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<VibeGateVerdictTag>,
    pub id: String,
    pub evidence_pack_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_request: Option<String>,
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

// ---------------------------------------------------------------------------
// 6. Merge Passport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergePassport {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<MergePassportTag>,
    pub id: String,
    pub verdict_id: String,
    pub repo: String,
    pub merge_request: String,
    pub head_sha: String,
    pub target_branch: String,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default = "true_default")]
    pub rebind_on_train: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
    pub signature: Signature,
}

// ---------------------------------------------------------------------------
// 7. Release Passport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Container,
    RustBinary,
    WasmModule,
    Deb,
    Rpm,
    Tarball,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployEnvironment {
    Dev,
    Staging,
    Canary,
    Prod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseRollbackPlan {
    pub strategy: String,
    #[serde(default)]
    pub tested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleasePassport {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<ReleasePassportTag>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    pub artifact_digest: String,
    pub artifact_kind: ArtifactKind,
    pub sbom_digest: String,
    pub provenance_digest: String,
    pub source_sha: String,
    pub build_logs_digest: String,
    #[serde(default)]
    pub allowed_environments: Vec<DeployEnvironment>,
    pub rollback_plan: ReleaseRollbackPlan,
    pub issued_at: DateTime<Utc>,
    pub signature: Signature,
}

// ---------------------------------------------------------------------------
// 8. Launch Ledger Entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    IntentDeclared,
    LeaseIssued,
    LeaseExpired,
    EvidencePackCreated,
    ReviewStarted,
    ReviewCompleted,
    VerdictIssued,
    MergePassportIssued,
    MergePassportConsumed,
    MergePassportInvalidated,
    ReleasePassportIssued,
    DeploymentStarted,
    DeploymentPromoted,
    RollbackInitiated,
    RollbackCompleted,
    HumanEscalationRequested,
    HumanDecisionRecorded,
    /// Wave 10 mint — a verified inbound webhook (today: GitHub
    /// `pull_request` events on `POST /events`). Dedicated kind so audit
    /// replay can distinguish webhook events from human decisions; the
    /// previous code path reused `HumanDecisionRecorded`.
    WebhookReceived,
    AutonomyPackEditProposed,
    AutonomyPackEditMerged,
    /// Wave 4 — Kill Bell engaged (global pause / break-glass).
    KillBellEngaged,
    /// Wave 4 — Kill Bell resumed (operator-initiated or TTL auto-arm).
    KillBellResumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchLedgerEntry {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<LaunchLedgerEntryTag>,
    pub id: String,
    pub kind: LedgerKind,
    pub subject_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: serde_json::Value,
    pub recorded_at: DateTime<Utc>,
    pub actor: String,
    pub signature: Signature,
}

// ---------------------------------------------------------------------------
// Schema-tag machinery
// ---------------------------------------------------------------------------
// Each typed object carries `schema: "vibegate.<thing>.v1"`. We enforce that
// via a zero-cost SchemaTag<T> wrapper that serializes as the literal string.

/// Marker trait: each object kind has a canonical schema id.
pub trait SchemaKind {
    const NAME: &'static str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IntentCardTag;
impl SchemaKind for IntentCardTag {
    const NAME: &'static str = "vibegate.intent_card.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CapabilityLeaseTag;
impl SchemaKind for CapabilityLeaseTag {
    const NAME: &'static str = "vibegate.capability_lease.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EvidencePackTag;
impl SchemaKind for EvidencePackTag {
    const NAME: &'static str = "vibegate.evidence_pack.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AgentApprovalReceiptTag;
impl SchemaKind for AgentApprovalReceiptTag {
    const NAME: &'static str = "vibegate.agent_approval_receipt.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VibeGateVerdictTag;
impl SchemaKind for VibeGateVerdictTag {
    const NAME: &'static str = "vibegate.gate_verdict.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MergePassportTag;
impl SchemaKind for MergePassportTag {
    const NAME: &'static str = "vibegate.merge_passport.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ReleasePassportTag;
impl SchemaKind for ReleasePassportTag {
    const NAME: &'static str = "vibegate.release_passport.v1";
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LaunchLedgerEntryTag;
impl SchemaKind for LaunchLedgerEntryTag {
    const NAME: &'static str = "vibegate.launch_ledger_entry.v1";
}

pub struct SchemaTag<T: SchemaKind>(std::marker::PhantomData<T>);

// Manually impl trait machinery so we don't require T: Debug/Clone/etc.
impl<T: SchemaKind> Default for SchemaTag<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}
impl<T: SchemaKind> Clone for SchemaTag<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: SchemaKind> Copy for SchemaTag<T> {}
impl<T: SchemaKind> std::fmt::Debug for SchemaTag<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SchemaTag<{}>", T::NAME)
    }
}
impl<T: SchemaKind> PartialEq for SchemaTag<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl<T: SchemaKind> Eq for SchemaTag<T> {}
impl<T: SchemaKind> std::hash::Hash for SchemaTag<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        T::NAME.hash(state);
    }
}

impl<T: SchemaKind> SchemaTag<T> {
    pub fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<T: SchemaKind> Serialize for SchemaTag<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(T::NAME)
    }
}

impl<'de, T: SchemaKind> Deserialize<'de> for SchemaTag<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s == T::NAME {
            Ok(SchemaTag::new())
        } else {
            Err(serde::de::Error::custom(format!(
                "schema mismatch: expected {}, got {}",
                T::NAME,
                s
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn risk_tier_categories() {
        assert!(RiskTier::R0.auto_merge_eligible());
        assert!(RiskTier::R2.auto_merge_eligible());
        assert!(!RiskTier::R3.auto_merge_eligible());
        assert!(RiskTier::R3.human_required());
        assert!(RiskTier::R5.human_required());
    }

    #[test]
    fn intent_card_round_trips() {
        let card = IntentCard {
            schema: SchemaTag::new(),
            id: "intent_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            agent_id: "builder.fix-bug".into(),
            repo: "org/proj".into(),
            target_branch: Some("main".into()),
            summary: "fix off-by-one".into(),
            linked_issue: None,
            estimated_risk: Some(RiskTier::R1),
            expected_changed_paths: vec!["src/lib.rs".into()],
            claims: vec!["adds regression test".into()],
            created_at: now(),
            signature: None,
        };
        let j = serde_json::to_string(&card).unwrap();
        assert!(j.contains("\"schema\":\"vibegate.intent_card.v1\""));
        let back: IntentCard = serde_json::from_str(&j).unwrap();
        assert_eq!(card, back);
    }

    #[test]
    fn schema_mismatch_rejected() {
        let j = r#"{"schema":"vibegate.wrong.v1","id":"intent_x","agent_id":"a","repo":"r","summary":"s","created_at":"2026-05-16T00:00:00Z"}"#;
        let err = serde_json::from_str::<IntentCard>(j).unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn evidence_pack_round_trips() {
        let pack = EvidencePack {
            schema: SchemaTag::new(),
            id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            intent_id: None,
            repo: "org/proj".into(),
            source_branch: "agent/x".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            policy_sha: "c".repeat(40),
            author_agent: Some("builder.x".into()),
            risk: RiskTier::R2,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast: ScanOutcome::Passed,
                dependency_scan: ScanOutcome::Passed,
                secret_scan: ScanOutcome::Passed,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            gate_receipts: vec![],
            evidence_digest: format!("sha256:00{}", "0".repeat(62)),
            created_at: now(),
            signature: None,
        };
        let j = serde_json::to_string(&pack).unwrap();
        let back: EvidencePack = serde_json::from_str(&j).unwrap();
        assert_eq!(pack, back);
    }

    #[test]
    fn receipt_decision_serializes_lowercase() {
        let r = AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: "aar_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            evidence_pack_id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            role: ReviewerRole::Security,
            agent_id: "reviewer-security.v1".into(),
            prompt_sha: None,
            provider: Some("openrouter".into()),
            model: Some("nvidia/nemotron-3-super-120b-a12b:free".into()),
            temperature: Some(0.0),
            seed: None,
            raw_response_sha: None,
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            decision: ReviewDecision::Block,
            reason: Some("sql injection".into()),
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: now(),
            signature: Signature::stub(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"role\":\"security\""));
        assert!(j.contains("\"decision\":\"block\""));
    }

    // -- Wave 2.4: CapabilityLease::permits ------------------------------

    fn lease_for(
        agent: &str,
        allowed: &[&str],
        denied_actions: &[&str],
        denied_paths: &[&str],
        ttl_secs: u32,
    ) -> CapabilityLease {
        let issued = now();
        CapabilityLease {
            schema: SchemaTag::new(),
            id: "lease_1".into(),
            intent_id: "intent_1".into(),
            agent_id: agent.into(),
            scope: LeaseScope {
                allowed_actions: allowed.iter().map(|s| (*s).to_string()).collect(),
                denied_actions: denied_actions.iter().map(|s| (*s).to_string()).collect(),
                allowed_write_refs: vec![],
                denied_paths: denied_paths.iter().map(|s| (*s).to_string()).collect(),
            },
            ttl_seconds: ttl_secs,
            issued_at: issued,
            expires_at: issued + chrono::Duration::seconds(ttl_secs as i64),
            policy_sha: "c".repeat(40),
            signature: Signature::stub(),
        }
    }

    #[test]
    fn permits_happy_path() {
        let l = lease_for(
            "builder.v1",
            &["mr.create", "evidence.write"],
            &[],
            &[],
            3600,
        );
        assert!(
            l.permits("mr.create", "builder.v1", &["src/foo.rs"], now())
                .is_ok()
        );
    }

    #[test]
    fn permits_rejects_expired_lease() {
        let l = lease_for("builder.v1", &["mr.create"], &[], &[], 0);
        let future = now() + chrono::Duration::seconds(60);
        let err = l
            .permits("mr.create", "builder.v1", &[], future)
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::Expired { .. }));
    }

    #[test]
    fn permits_rejects_wrong_agent() {
        let l = lease_for("builder.v1", &["mr.create"], &[], &[], 3600);
        let err = l.permits("mr.create", "hacker.v1", &[], now()).unwrap_err();
        assert!(matches!(err, LeaseDenied::AgentIdMismatch { .. }));
    }

    #[test]
    fn permits_rejects_explicit_denied_action() {
        let l = lease_for(
            "builder.v1",
            &["mr.create", "approve.own"],
            &["approve.own"],
            &[],
            3600,
        );
        let err = l
            .permits("approve.own", "builder.v1", &[], now())
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::ActionDenied(_)));
    }

    #[test]
    fn permits_rejects_action_not_in_allowlist() {
        let l = lease_for("builder.v1", &["mr.create"], &[], &[], 3600);
        let err = l
            .permits("deploy.prod", "builder.v1", &[], now())
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::ActionNotAllowed(_)));
    }

    #[test]
    fn permits_rejects_denied_path() {
        let l = lease_for(
            "builder.v1",
            &["mr.create"],
            &[],
            &[".jeryu/autonomy/**", "secrets/**"],
            3600,
        );
        let err = l
            .permits(
                "mr.create",
                "builder.v1",
                &["src/foo.rs", ".jeryu/autonomy/policies/risk.yml"],
                now(),
            )
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::PathDenied { .. }));
    }

    #[test]
    fn permits_allows_paths_not_in_denied_list() {
        let l = lease_for(
            "builder.v1",
            &["mr.create"],
            &[],
            &[".jeryu/autonomy/**"],
            3600,
        );
        assert!(
            l.permits("mr.create", "builder.v1", &["src/main.rs"], now())
                .is_ok()
        );
    }

    /// Wave 10 mint — `WebhookReceived` exists as a distinct enum variant
    /// (not aliased to `HumanDecisionRecorded`) and serializes through the
    /// usual snake_case path.
    #[test]
    fn ledger_kind_includes_webhook_received() {
        // Variant exists.
        let kind = LedgerKind::WebhookReceived;
        // Disjoint from HumanDecisionRecorded.
        assert_ne!(kind, LedgerKind::HumanDecisionRecorded);
        // Serializes to snake_case JSON literal.
        let j = serde_json::to_string(&kind).expect("serializes");
        assert_eq!(j, "\"webhook_received\"", "got {j}");
        // Round-trips back through deserialize.
        let back: LedgerKind = serde_json::from_str(&j).expect("deserializes");
        assert_eq!(back, LedgerKind::WebhookReceived);
        // Human-decision entries keep their canonical ledger spelling.
        let human = serde_json::to_string(&LedgerKind::HumanDecisionRecorded).unwrap();
        assert_eq!(human, "\"human_decision_recorded\"");
    }

    #[test]
    fn permits_empty_allowlist_permits_any_action() {
        let l = lease_for("builder.v1", &[], &[], &[], 3600);
        assert!(
            l.permits("any.action", "builder.v1", &[], now()).is_ok(),
            "empty allowlist means 'no allowlist constraint'; \
             explicit denied_actions still bite"
        );
    }
}
