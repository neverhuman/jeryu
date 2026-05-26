//! Owner: Interactive TUI subsystem - Evidence lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::evidence::data`
//! Invariants: Pure projection from `TuiReadModel` to `EvidenceLensInput`.
//!             No I/O. Render layer reads only the resulting struct.
//!             Proof timeline + entity proof graph land in U21 proper;
//!             this first-cut only exposes the attention-derived count.

use crate::api::read_model::TuiReadModel;

#[derive(Debug, Clone)]
pub struct EvidenceLensInput {
    pub attention_count: usize,
    pub event_cursor: u64,
}

impl EvidenceLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        Self {
            attention_count: model.attention.len(),
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

    fn attention(id: &str) -> AttentionItem {
        let now = Utc::now();
        AttentionItem {
            id: id.into(),
            severity: Severity::Info,
            title: format!("title-{id}"),
            why_it_matters: "why".into(),
            entity: EntityRef::new(EntityKind::Project, id),
            evidence: Vec::new(),
            recommended_actions: Vec::new(),
            created_at: now,
            last_seen_at: now,
        }
    }

    #[test]
    fn select_from_default_read_model_returns_zero_attention() {
        let model = TuiReadModel::default();
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.attention_count, 0);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 3456,
            ..Default::default()
        };
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 3456);
    }

    #[test]
    fn select_counts_attention_items() {
        let mut model = TuiReadModel::default();
        for i in 0..3 {
            model.attention.push(attention(&format!("a{i}")));
        }
        let input = EvidenceLensInput::from_read_model(&model);
        assert_eq!(input.attention_count, 3);
    }
}
