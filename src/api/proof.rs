//! Owner: TUI Control-Plane API - proof / evidence reference contract
//! Proof: `cargo test -p jeryu --lib api::proof`
//! Invariants: ProofRef/EvidenceRef identify durable proof rows; ProofQuery
//!             cannot smuggle raw SQL into projections.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::entity::{EntityKind, EntityRef};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProofConfidence {
    Meas,
    Struct,
    Hist,
    Heur,
    Miss,
    Stale,
}

impl ProofConfidence {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Meas => "MEAS",
            Self::Struct => "STRUCT",
            Self::Hist => "HIST",
            Self::Heur => "HEUR",
            Self::Miss => "MISS",
            Self::Stale => "STALE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofRef {
    pub id: String,
    pub entity: EntityRef,
    pub kind: String,
    pub recorded_at: DateTime<Utc>,
    pub actor: Option<String>,
    pub confidence: ProofConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProofTimeline {
    pub entries: Vec<ProofRef>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProofQuery {
    pub entity_kind: Option<EntityKind>,
    pub entity_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub actor: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

impl ProofQuery {
    pub fn effective_limit(&self) -> u32 {
        self.limit.map(|n| n.min(500)).unwrap_or(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_badges_match_plan_language() {
        assert_eq!(ProofConfidence::Meas.badge(), "MEAS");
        assert_eq!(ProofConfidence::Miss.badge(), "MISS");
    }

    #[test]
    fn proof_query_caps_limit() {
        let q = ProofQuery {
            limit: Some(10_000),
            ..ProofQuery::default()
        };
        assert_eq!(q.effective_limit(), 500);
    }

    #[test]
    fn proof_query_default_limit_is_one_hundred() {
        let q = ProofQuery::default();
        assert_eq!(q.effective_limit(), 100);
    }

    #[test]
    fn proof_ref_serde_roundtrip() {
        let pr = ProofRef {
            id: "p-1".into(),
            entity: EntityRef::new(EntityKind::Job, "j-1"),
            kind: "pipeline_passed".into(),
            recorded_at: Utc::now(),
            actor: Some("system".into()),
            confidence: ProofConfidence::Meas,
        };
        let json = serde_json::to_string(&pr).unwrap();
        let back: ProofRef = serde_json::from_str(&json).unwrap();
        assert_eq!(pr, back);
    }
}
