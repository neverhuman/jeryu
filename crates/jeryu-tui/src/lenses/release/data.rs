//! Release lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`ReleaseLensInput`].
//! No I/O. Projects release candidates from the read model's release dashboard:
//! per-candidate rows (gate/stage/SBOM) plus the production-posture rollup from
//! the dashboard summary.

use jeryu_readmodel::{
    HealthLevel, PromotionStage, ReleaseGate, ReleaseItem, SbomStatus, TuiReadModel,
};

/// One release-candidate row.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseRow {
    pub release_id: String,
    pub label: String,
    pub candidate_sha: String,
    pub gate: ReleaseGate,
    pub stage: PromotionStage,
    pub sbom: SbomStatus,
    pub rollback_target: Option<String>,
}

impl ReleaseRow {
    fn from_item(item: &ReleaseItem) -> Self {
        Self {
            release_id: item.release_id.clone(),
            label: item.label.clone(),
            candidate_sha: item.candidate_sha.clone(),
            gate: item.gate,
            stage: item.stage,
            sbom: item.sbom,
            rollback_target: item.rollback_target.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseLensInput {
    pub safe_to_release: bool,
    pub production_health: HealthLevel,
    pub candidate_ready: bool,
    pub canary_passing: bool,
    pub rows: Vec<ReleaseRow>,
    pub event_cursor: u64,
}

impl Default for ReleaseLensInput {
    fn default() -> Self {
        Self {
            safe_to_release: false,
            production_health: HealthLevel::Unknown,
            candidate_ready: false,
            canary_passing: false,
            rows: Vec::new(),
            event_cursor: 0,
        }
    }
}

impl ReleaseLensInput {
    pub fn from_read_model(model: &TuiReadModel) -> Self {
        let summary = model.release.summary.as_ref();
        let rows: Vec<ReleaseRow> = model
            .release
            .items
            .iter()
            .map(ReleaseRow::from_item)
            .collect();
        Self {
            safe_to_release: model.mission.safe_to_release,
            production_health: summary
                .map(|s| s.production_health)
                .unwrap_or(HealthLevel::Unknown),
            candidate_ready: summary.map(|s| s.candidate_ready).unwrap_or(false),
            canary_passing: summary.map(|s| s.canary_passing).unwrap_or(false),
            rows,
            event_cursor: model.event_cursor,
        }
    }

    /// Count of candidates blocked by a failing gate — drives header emphasis.
    pub fn blocked(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.gate == ReleaseGate::Blocked)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_readmodel::sample_read_model;

    #[test]
    fn empty_from_default_read_model() {
        let input = ReleaseLensInput::from_read_model(&TuiReadModel::default());
        assert!(!input.safe_to_release);
        assert!(input.rows.is_empty());
        assert_eq!(input.blocked(), 0);
        assert_eq!(input.production_health, HealthLevel::Unknown);
        assert_eq!(input.event_cursor, 0);
    }

    #[test]
    fn projects_candidates_from_sample() {
        let model = sample_read_model();
        let input = ReleaseLensInput::from_read_model(&model);
        assert_eq!(input.rows.len(), 2);
        assert_eq!(input.rows[0].release_id, "rel-1");
        assert_eq!(input.rows[0].gate, ReleaseGate::Ready);
        assert_eq!(input.rows[0].stage, PromotionStage::Canary);
        assert_eq!(input.rows[0].sbom, SbomStatus::Verified);
        assert_eq!(input.rows[1].gate, ReleaseGate::Blocked);
        assert_eq!(input.rows[1].sbom, SbomStatus::Missing);
        assert_eq!(input.blocked(), 1);
        assert!(input.candidate_ready);
        assert!(input.canary_passing);
        assert_eq!(input.production_health, HealthLevel::Healthy);
        assert_eq!(input.event_cursor, 42);
    }
}
