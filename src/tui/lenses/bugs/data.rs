//! Owner: Interactive TUI subsystem - Bugs lens data selector
//! Proof: `cargo test -p jeryu --lib tui::lenses::bugs::data`
//! Invariants: Pure projection from `TuiReadModel` to `BugsLensInput`. No I/O.
//!             The render layer reads only the resulting owned struct (no
//!             lifetimes), so the lens triages bugs/blockers from the mission
//!             top blocker plus the ranked attention queue.

use crate::api::entity::Severity;
use crate::api::read_model::TuiReadModel;

/// One triage row in the bugs pane, projected from an `AttentionItem`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BugRow {
    pub severity: Severity,
    pub title: String,
    pub why_it_matters: String,
}

#[derive(Debug, Clone, Default)]
pub struct BugsLensInput {
    pub open_capsules: u32,
    /// Failed CI jobs right now — a primary source of fresh bugs.
    pub failed_jobs: u32,
    /// Human-readable summary of the single most important blocker, if any.
    pub top_blocker: Option<String>,
    /// Severity of the top blocker, used to color the header.
    pub top_blocker_severity: Option<Severity>,
    /// Ranked triage rows projected from the attention queue.
    pub rows: Vec<BugRow>,
    pub event_cursor: u64,
}

impl BugsLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let (top_blocker, top_blocker_severity) = match &model.mission.top_blocker {
            Some(b) => (Some(b.summary.clone()), Some(b.severity)),
            None => (None, None),
        };
        let rows = model
            .attention
            .iter()
            .map(|item| BugRow {
                severity: item.severity,
                title: item.title.clone(),
                why_it_matters: item.why_it_matters.clone(),
            })
            .collect();
        Self {
            open_capsules: model.mission.open_capsules,
            failed_jobs: model.mission.failed_jobs,
            top_blocker,
            top_blocker_severity,
            rows,
            event_cursor: model.event_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{BlockerSummary, EntityKind, EntityRef};
    use crate::api::read_model::{AttentionItem, MissionSnapshot};
    use chrono::Utc;

    fn attention(sev: Severity, title: &str, why: &str) -> AttentionItem {
        AttentionItem {
            id: title.into(),
            severity: sev,
            title: title.into(),
            why_it_matters: why.into(),
            entity: EntityRef::new(EntityKind::Bug, title),
            evidence: Vec::new(),
            recommended_actions: Vec::new(),
            created_at: Utc::now(),
            last_seen_at: Utc::now(),
        }
    }

    #[test]
    fn select_from_default_read_model_is_empty() {
        let input = BugsLensInput::from_read_model(&TuiReadModel::default());
        assert_eq!(input.open_capsules, 0);
        assert_eq!(input.failed_jobs, 0);
        assert!(input.top_blocker.is_none());
        assert!(input.top_blocker_severity.is_none());
        assert!(input.rows.is_empty());
    }

    #[test]
    fn select_preserves_event_cursor() {
        let model = TuiReadModel {
            event_cursor: 42,
            ..Default::default()
        };
        let input = BugsLensInput::from_read_model(&model);
        assert_eq!(input.event_cursor, 42);
    }

    #[test]
    fn select_projects_blocker_and_attention_rows() {
        let model = TuiReadModel {
            mission: MissionSnapshot {
                failed_jobs: 3,
                top_blocker: Some(BlockerSummary {
                    kind: "merge_gate".into(),
                    severity: Severity::Critical,
                    summary: "release gate failing on jeryu".into(),
                    entity: None,
                    recommended_action: None,
                }),
                ..Default::default()
            },
            attention: vec![
                attention(Severity::Error, "flaky test", "blocks merge of MR 42"),
                attention(
                    Severity::Warning,
                    "stale capsule",
                    "evidence may be outdated",
                ),
            ],
            ..Default::default()
        };
        let input = BugsLensInput::from_read_model(&model);
        assert_eq!(input.failed_jobs, 3);
        assert_eq!(
            input.top_blocker.as_deref(),
            Some("release gate failing on jeryu")
        );
        assert_eq!(input.top_blocker_severity, Some(Severity::Critical));
        assert_eq!(input.rows.len(), 2);
        assert_eq!(input.rows[0].title, "flaky test");
        assert_eq!(input.rows[0].severity, Severity::Error);
        assert_eq!(input.rows[1].why_it_matters, "evidence may be outdated");
    }
}
