//! Canonical object 4: the Agent Approval Receipt — a single reviewer agent's
//! signed verdict on an evidence pack, including its findings and the
//! provenance (model / prompt / seed) of the review.

use super::common::{ReviewDecision, ReviewerRole, Severity};
use super::schema_tag::{AgentApprovalReceiptTag, SchemaTag};
use super::true_default;
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
