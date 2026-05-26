use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::{
    api::{
        actions::{ActionReceipt, ActionStatus},
        entity::EntityRef,
        events::TuiEvent,
        inspection::{EventPage, InspectionEnvelope, ProofDetail},
    },
    tui::{
        lenses::evidence::bundle::{self, RedactedBundlePreview},
        theme::ProofConfidence,
    },
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceSearch {
    pub query: String,
    pub selected_entity: Option<EntityRef>,
}

#[derive(Debug, Clone, Copy)]
pub struct EvidenceLensInput<'a> {
    pub generated_at: DateTime<Utc>,
    pub search: &'a EvidenceSearch,
    pub event_page: &'a EventPage,
    pub proofs: &'a [ProofDetail],
    pub receipts: &'a [ActionReceipt],
}

pub fn select_evidence_lens_input<'a>(
    events: &'a InspectionEnvelope<EventPage>,
    proofs: &'a [ProofDetail],
    receipts: &'a [ActionReceipt],
    search: &'a EvidenceSearch,
) -> EvidenceLensInput<'a> {
    EvidenceLensInput {
        generated_at: events.generated_at,
        search,
        event_page: &events.data,
        proofs,
        receipts,
    }
}

impl EvidenceLensInput<'_> {
    pub fn proof_hits(self) -> Vec<ProofHit> {
        let mut hits = BTreeMap::new();
        for proof in self.proofs {
            if !proof_matches_entity(proof, &self.search.selected_entity) {
                continue;
            }
            let hit = ProofHit {
                proof_id: proof.proof_id.clone(),
                status: proof.status.clone(),
                summary: proof.summary.clone(),
                entity: proof.entity.as_ref().map(|detail| detail.entity.clone()),
                generated_at: proof.generated_at,
                evidence_refs: proof.evidence_refs.clone(),
                confidence: confidence_for_status(&proof.status),
                source: ProofHitSource::ProofDetail,
            };
            hits.insert(hit.proof_id.clone(), hit);
        }

        for event in &self.event_page.events {
            if !event_matches_entity(event, &self.search.selected_entity) {
                continue;
            }
            for proof_id in &event.evidence_refs {
                hits.entry(proof_id.clone()).or_insert_with(|| ProofHit {
                    proof_id: proof_id.clone(),
                    status: "missing".into(),
                    summary: format!("Referenced by {}", event.kind.label()),
                    entity: Some(event.entity.clone()),
                    generated_at: event.timestamp,
                    evidence_refs: Vec::new(),
                    confidence: ProofConfidence::Missing,
                    source: ProofHitSource::EventReference,
                });
            }
        }

        let query = self.search.query.trim().to_ascii_lowercase();
        hits.into_values()
            .filter(|hit| hit.matches_query(&query))
            .collect()
    }

    pub fn receipt_hits(self) -> Vec<ReceiptHit> {
        let query = self.search.query.trim().to_ascii_lowercase();
        self.receipts
            .iter()
            .filter(|receipt| receipt_matches_entity(receipt, &self.search.selected_entity))
            .map(ReceiptHit::from)
            .filter(|hit| hit.matches_query(&query))
            .collect()
    }

    pub fn bundle_preview(self) -> RedactedBundlePreview {
        bundle::build_redacted_bundle_preview(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofHit {
    pub proof_id: String,
    pub status: String,
    pub summary: String,
    pub entity: Option<EntityRef>,
    pub generated_at: DateTime<Utc>,
    pub evidence_refs: Vec<String>,
    pub confidence: ProofConfidence,
    pub source: ProofHitSource,
}

impl ProofHit {
    fn matches_query(&self, query: &str) -> bool {
        query.is_empty()
            || contains_query(&self.proof_id, query)
            || contains_query(&self.status, query)
            || contains_query(&self.summary, query)
            || self
                .entity
                .as_ref()
                .is_some_and(|entity| contains_query(&entity.display(), query))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofHitSource {
    ProofDetail,
    EventReference,
}

impl ProofHitSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::ProofDetail => "proof",
            Self::EventReference => "event",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptHit {
    pub receipt_id: String,
    pub action_id: String,
    pub status: ActionStatus,
    pub summary: String,
    pub affected_entity: Option<EntityRef>,
    pub evidence_created: Vec<String>,
    pub accepted_at: DateTime<Utc>,
    pub dry_run: bool,
}

impl ReceiptHit {
    pub fn status_label(&self) -> &'static str {
        match self.status {
            ActionStatus::Completed => "completed",
            ActionStatus::Accepted => "accepted",
            ActionStatus::RequiresApproval => "requires_approval",
            ActionStatus::Rejected => "rejected",
            ActionStatus::Failed => "failed",
            ActionStatus::Cancelled => "cancelled",
        }
    }

    fn matches_query(&self, query: &str) -> bool {
        query.is_empty()
            || contains_query(&self.receipt_id, query)
            || contains_query(&self.action_id, query)
            || contains_query(self.status_label(), query)
            || contains_query(&self.summary, query)
            || self
                .affected_entity
                .as_ref()
                .is_some_and(|entity| contains_query(&entity.display(), query))
    }
}

impl From<&ActionReceipt> for ReceiptHit {
    fn from(receipt: &ActionReceipt) -> Self {
        Self {
            receipt_id: receipt.receipt_id.clone(),
            action_id: receipt.action_id.clone(),
            status: receipt.status,
            summary: receipt.summary.clone(),
            affected_entity: receipt.affected_entity.clone(),
            evidence_created: receipt.evidence_created.clone(),
            accepted_at: receipt.accepted_at,
            dry_run: receipt.dry_run,
        }
    }
}

fn confidence_for_status(status: &str) -> ProofConfidence {
    let status = status.to_ascii_lowercase();
    if status.contains("stale") || status.contains("expired") {
        ProofConfidence::Stale
    } else if status.contains("heur") || status.contains("inferred") {
        ProofConfidence::Heuristic
    } else if status == "missing" || status == "unknown" {
        ProofConfidence::Missing
    } else if status.contains("verified")
        || status.contains("accepted")
        || status.contains("complete")
        || status.contains("pass")
        || status == "ok"
    {
        ProofConfidence::Measured
    } else {
        ProofConfidence::Unverified
    }
}

fn proof_matches_entity(proof: &ProofDetail, selected: &Option<EntityRef>) -> bool {
    selected.as_ref().is_none_or(|selected| {
        proof
            .entity
            .as_ref()
            .is_some_and(|detail| detail.entity == *selected)
    })
}

fn event_matches_entity(event: &TuiEvent, selected: &Option<EntityRef>) -> bool {
    selected
        .as_ref()
        .is_none_or(|selected| event.entity == *selected || event.parent.as_ref() == Some(selected))
}

fn receipt_matches_entity(receipt: &ActionReceipt, selected: &Option<EntityRef>) -> bool {
    selected.as_ref().is_none_or(|selected| {
        receipt.affected_entity.as_ref() == Some(selected)
            || receipt
                .evidence_created
                .iter()
                .any(|proof_id| proof_id.contains(&selected.id))
    })
}

fn contains_query(value: &str, query: &str) -> bool {
    value.to_ascii_lowercase().contains(query)
}
