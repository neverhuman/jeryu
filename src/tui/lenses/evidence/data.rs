//! Owner: Interactive TUI subsystem - Evidence lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::evidence::data`
//! Invariants: Pure projection from `TuiReadModel` to `EvidenceLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Projects the proof ledger: capsule counts from the mission
//!             snapshot plus the evidence references gathered across every
//!             attention item.

use crate::api::read_model::TuiReadModel;

/// One row in the evidence ledger: which attention source surfaced it and the
/// evidence reference string itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRow {
    /// Title of the attention item this evidence reference came from.
    pub source_title: String,
    /// The evidence reference (capsule id, artifact path, event ref, …).
    pub evidence_ref: String,
}

#[derive(Debug, Clone, Default)]
pub struct EvidenceLensInput {
    /// Total recorded evidence capsules (from the mission snapshot).
    pub evidence_count: u32,
    /// Capsules still open / awaiting resolution.
    pub open_capsules: u32,
    /// Number of attention items contributing references.
    pub attention_count: usize,
    /// Flattened evidence ledger projected from `model.attention`.
    pub rows: Vec<EvidenceRow>,
    pub event_cursor: u64,
}

impl EvidenceLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let rows: Vec<EvidenceRow> = model
            .attention
            .iter()
            .flat_map(|item| {
                item.evidence.iter().map(move |ev| EvidenceRow {
                    source_title: item.title.clone(),
                    evidence_ref: ev.clone(),
                })
            })
            .collect();

        Self {
            evidence_count: model.mission.evidence_count,
            open_capsules: model.mission.open_capsules,
            attention_count: model.attention.len(),
            rows,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{EntityKind, EntityRef, Severity};
    use crate::api::read_model::AttentionItem;
    use chrono::Utc;

    fn attention(id: &str, evidence: Vec<&str>) -> AttentionItem {
        let now = Utc::now();
        AttentionItem {
            id: id.into(),
            severity: Severity::Info,
            title: format!("title-{id}"),
            why_it_matters: "why".into(),
            entity: EntityRef::new(EntityKind::Project, id),
            evidence: evidence.into_iter().map(String::from).collect(),
            recommended_actions: Vec::new(),
            created_at: now,
            last_seen_at: now,
        }
    }

    #[test]
    fn select_from_default_read_model_is_empty() {
        let model = TuiReadModel::default();
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.evidence_count, 0);
        assert_eq!(input.open_capsules, 0);
        assert_eq!(input.attention_count, 0);
        assert!(input.rows.is_empty());
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn select_projects_capsule_counts_from_mission() {
        let mut model = TuiReadModel::default();
        model.mission.evidence_count = 9;
        model.mission.open_capsules = 4;
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.evidence_count, 9);
        assert_eq!(input.open_capsules, 4);
    }

    #[test]
    fn select_flattens_evidence_refs_across_attention() {
        let mut model = TuiReadModel::default();
        model
            .attention
            .push(attention("a0", vec!["cap://1", "cap://2"]));
        model.attention.push(attention("a1", vec!["cap://3"]));
        model.attention.push(attention("a2", vec![]));
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.attention_count, 3);
        assert_eq!(input.rows.len(), 3);
        assert_eq!(input.rows[0].source_title, "title-a0");
        assert_eq!(input.rows[0].evidence_ref, "cap://1");
        assert_eq!(input.rows[2].evidence_ref, "cap://3");
    }
}
