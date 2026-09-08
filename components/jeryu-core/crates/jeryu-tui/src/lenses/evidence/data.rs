//! Evidence lens data selector.
//!
//! Invariants: pure projection from [`TuiReadModel`] to [`EvidenceLensInput`].
//! No I/O. Projects the proof ledger plus codegraph/oracle impact-pack
//! evidence from the read model.

use jeryu_readmodel::{
    CodegraphEvidenceItem, EntityRef, EvidenceItem, GateDecision, ToolBuildOpportunityItem,
    TuiReadModel,
};

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

#[derive(Debug, Clone, PartialEq)]
pub struct CodegraphEvidenceRow {
    pub query_id: String,
    pub tool: String,
    pub symbol: String,
    pub schema_version: u32,
    pub references: u32,
    pub required_reads: Vec<String>,
    pub proof_lanes: Vec<String>,
    pub miss: Option<String>,
}

impl CodegraphEvidenceRow {
    fn from_item(item: &CodegraphEvidenceItem) -> Self {
        Self {
            query_id: item.query_id.clone(),
            tool: item.tool.clone(),
            symbol: item.symbol.clone(),
            schema_version: item.schema_version,
            references: item.references,
            required_reads: item.required_reads.clone(),
            proof_lanes: item.proof_lanes.clone(),
            miss: item.miss.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolBuildOpportunityRow {
    pub cluster_id: String,
    pub repo_id: String,
    pub score: u64,
    pub occurrences: usize,
    pub file_count: usize,
    pub language: String,
    pub suggested_proof_lane: String,
}

impl ToolBuildOpportunityRow {
    fn from_item(item: &ToolBuildOpportunityItem) -> Self {
        Self {
            cluster_id: item.cluster_id.clone(),
            repo_id: item.repo_id.clone(),
            score: item.score,
            occurrences: item.occurrences,
            file_count: item.file_count,
            language: item.language.clone(),
            suggested_proof_lane: item.suggested_proof_lane.clone(),
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
    pub codegraph_rows: Vec<CodegraphEvidenceRow>,
    pub tool_build_rows: Vec<ToolBuildOpportunityRow>,
    pub codegraph_schema_version: Option<u32>,
    pub codegraph_misses: u32,
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
        let codegraph_rows: Vec<CodegraphEvidenceRow> = model
            .codegraph
            .items
            .iter()
            .map(CodegraphEvidenceRow::from_item)
            .collect();
        let tool_build_rows: Vec<ToolBuildOpportunityRow> = model
            .codegraph
            .tool_build_opportunities
            .iter()
            .map(ToolBuildOpportunityRow::from_item)
            .collect();
        Self {
            total_capsules: summary
                .map(|s| s.total_capsules)
                .unwrap_or(model.mission.evidence_count),
            open_capsules: summary
                .map(|s| s.open_capsules)
                .unwrap_or(model.mission.open_capsules),
            rows,
            codegraph_rows,
            tool_build_rows,
            codegraph_schema_version: model.codegraph.summary.as_ref().map(|s| s.schema_version),
            codegraph_misses: model
                .codegraph
                .summary
                .as_ref()
                .map(|s| s.miss_count)
                .unwrap_or_else(|| model.codegraph.misses()),
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
        assert!(input.codegraph_rows.is_empty());
        assert!(input.tool_build_rows.is_empty());
        assert_eq!(input.codegraph_schema_version, None);
        assert_eq!(input.codegraph_misses, 0);
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
        assert_eq!(input.codegraph_rows.len(), 1);
        assert_eq!(input.codegraph_rows[0].tool, "codegraph.query");
        assert_eq!(input.codegraph_rows[0].schema_version, 2);
        assert_eq!(
            input.codegraph_rows[0].proof_lanes,
            vec!["codegraph-oracle", "agent-runs"]
        );
        assert_eq!(input.tool_build_rows.len(), 1);
        assert_eq!(
            input.tool_build_rows[0].cluster_id,
            "toolbuild-agent-runner"
        );
        assert_eq!(input.tool_build_rows[0].repo_id, "core/api");
        assert_eq!(input.tool_build_rows[0].score, 91);
        assert_eq!(input.tool_build_rows[0].occurrences, 5);
        assert_eq!(input.tool_build_rows[0].file_count, 3);
        assert_eq!(input.tool_build_rows[0].language, "rust");
        assert_eq!(
            input.tool_build_rows[0].suggested_proof_lane,
            "bash ops/ci/codegraph-tool-build.sh"
        );
        assert_eq!(input.codegraph_misses, 0);
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
