//! Canonical object 8: the Launch Ledger Entry — one signed, append-only record
//! in the launch ledger, plus the closed set of ledger event kinds.

use super::schema_tag::{LaunchLedgerEntryTag, SchemaTag};
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    /// A verified inbound webhook (a forge `pull_request` event on
    /// `POST /events`). Dedicated kind so audit replay can distinguish webhook
    /// events from human decisions.
    WebhookReceived,
    AutonomyPackEditProposed,
    AutonomyPackEditMerged,
    /// Kill Bell engaged (global pause / break-glass).
    KillBellEngaged,
    /// Kill Bell resumed (operator-initiated or TTL auto-arm).
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
