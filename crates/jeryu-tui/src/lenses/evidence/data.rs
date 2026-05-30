//! Evidence lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`EvidenceLensInput`].
//! No I/O. Projects the proof ledger: capsule receipts (capsule id, the entity
//! they cover, the gate decision they back) from the read model's evidence
//! dashboard, plus the open/total capsule counts from the dashboard summary.

use jeryu_readmodel::{EntityRef, EvidenceItem, GateDecision, TuiReadModel};

/// One row in the proof ledger: a receipt and the decision it justified.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRow {
    pub capsule_id: String,
    pub label: String,
    pub entity: EntityRef,
    pub decision: GateDecision,
    pub redacted: bool,
}

impl EvidenceRow {
    fn from_item(item: &EvidenceItem) -> Self {
        Self {
            capsule_id: item.capsule_id.clone(),
            label: item.label.clone(),
            entity: item.entity.clone(),
            decision: item.decision,
            redacted: item.redacted,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EvidenceLensInput {
    /// Total recorded capsules (from the dashboard summary).
    pub total_capsules: u32,
    /// Capsules still open / awaiting resolution.
    pub open_capsules: u32,
    /// Proof-receipt rows projected from the dashboard items.
    pub rows: Vec<EvidenceRow>,
    pub event_cursor: u64,
}

impl EvidenceLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.evidence.summary.as_ref();
        let rows: Vec<EvidenceRow> = model
            .evidence
            .items
            .iter()
            .map(EvidenceRow::from_item)
            .collect();
        Self {
            total_capsules: summary
                .map(|s| s.total_capsules)
                .unwrap_or(model.mission.evidence_count),
            open_capsules: summary
                .map(|s| s.open_capsules)
                .unwrap_or(model.mission.open_capsules),
            rows,
            event_cursor: model.event_cursor,
        }
    }

    /// Count of receipts whose gate denied the action — drives the alert.
    pub fn denied(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.decision == GateDecision::Deny)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn empty_from_default_read_model() {
        let input = EvidenceLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.total_capsules, 0);
        assert_eq!(input.open_capsules, 0);
        assert!(input.rows.is_empty());
        assert_eq!(input.denied(), 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_receipts_from_sample() {
        let model = sample_read_model();
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.total_capsules, 17);
        assert_eq!(input.open_capsules, 5);
        assert_eq!(input.rows.len(), 2);
        assert_eq!(input.rows[0].capsule_id, "cap-17");
        assert_eq!(input.rows[0].decision, GateDecision::Allow);
        assert_eq!(input.rows[1].decision, GateDecision::Deny);
        assert!(input.rows[1].redacted);
        assert_eq!(input.denied(), 1);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn falls_back_to_mission_counts_without_summary() {
        let mut model = TuiReadModel::default();
        model.mission.evidence_count = 9;
        model.mission.open_capsules = 4;
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.total_capsules, 9);
        assert_eq!(input.open_capsules, 4);
    }
}
