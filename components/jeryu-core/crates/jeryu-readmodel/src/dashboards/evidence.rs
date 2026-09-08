//! Evidence dashboard contract — proof receipts / gate decisions.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable". Each
//! item is a proof receipt: a recorded capsule for some control-plane entity,
//! optionally carrying the gate decision it justified.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entity::{EntityKind, EntityRef};
use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceSnapshot {
    pub items: Vec<EvidenceItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<EvidenceSummary>,
}

impl EvidenceSnapshot {
    /// Count of receipts whose gate decision denied the action.
    pub fn denied(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.decision == GateDecision::Deny)
            .count() as u32
    }
}

/// One proof receipt: an evidence capsule recorded against an entity, plus the
/// gate decision it justified (if any).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceItem {
    /// Capsule / receipt id.
    pub capsule_id: String,
    pub label: String,
    /// The entity this evidence was recorded for.
    pub entity: EntityRef,
    /// The gate decision this receipt backs.
    pub decision: GateDecision,
    pub recorded_at: Option<DateTime<Utc>>,
    /// True when the capsule body is redacted (secret material removed).
    pub redacted: bool,
}

impl EvidenceItem {
    pub fn new(capsule_id: impl Into<String>, entity: EntityRef, decision: GateDecision) -> Self {
        Self {
            capsule_id: capsule_id.into(),
            label: String::new(),
            entity,
            decision,
            recorded_at: None,
            redacted: false,
        }
    }
}

impl Default for EvidenceItem {
    fn default() -> Self {
        Self::new(
            "unknown",
            EntityRef::new(EntityKind::Evidence, "unknown"),
            GateDecision::Recorded,
        )
    }
}

/// The decision a gate reached, backed by the receipt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    /// Gate allowed the action.
    Allow,
    /// Gate denied the action.
    Deny,
    /// Decision pending / awaiting input.
    Pending,
    /// Pure receipt with no gate verdict.
    Recorded,
}

impl GateDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Pending => "pending",
            Self::Recorded => "recorded",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct EvidenceSummary {
    pub total_capsules: u32,
    pub open_capsules: u32,
    pub denied_count: u32,
    pub redacted_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = EvidenceSnapshot::default();
        assert!(d.items.is_empty());
        assert!(d.freshness.is_none());
        assert_eq!(d.denied(), 0);
    }

    #[test]
    fn gate_decision_round_trips() {
        let json = serde_json::to_string(&GateDecision::Deny).unwrap();
        assert_eq!(json, "\"deny\"");
        let back: GateDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, GateDecision::Deny);
    }

    #[test]
    fn denied_counts_deny_receipts() {
        let d = EvidenceSnapshot {
            items: vec![
                EvidenceItem::new(
                    "cap-1",
                    EntityRef::new(EntityKind::PullRequest, "101"),
                    GateDecision::Deny,
                ),
                EvidenceItem::new(
                    "cap-2",
                    EntityRef::new(EntityKind::PullRequest, "102"),
                    GateDecision::Allow,
                ),
            ],
            freshness: None,
            summary: None,
        };
        assert_eq!(d.denied(), 1);
    }

    #[test]
    fn dashboard_serde_roundtrip() {
        let d = EvidenceSnapshot::default();
        let json = serde_json::to_string(&d).unwrap();
        let back: EvidenceSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
