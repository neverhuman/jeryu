//! Canonical object 1: the Intent Card — an agent's declared intent to open a
//! pull request, signed and bound to a repo / target branch.

use super::common::RiskTier;
use super::schema_tag::{IntentCardTag, SchemaTag};
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
