//! Canonical object 5: the VibeGate Verdict — the judge's fused, signed
//! decision (AllowMerge / RequireHuman / Reject) bound to a head SHA, carrying
//! the hard stops, required reviews, and the receipts that backed it.

use super::common::{GateDecision, ReviewDecision, ReviewerRole, RiskTier};
use super::schema_tag::{SchemaTag, VibeGateVerdictTag};
use super::true_default;
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// Pull request identifier bound to the evidence pack head SHA.
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
