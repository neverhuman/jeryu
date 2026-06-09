//! Codegraph/oracle dashboard contract - query evidence surfaced to operators.
//!
//! Pure data; freshness carried alongside; default = "empty/unavailable".
//! Rows summarize the impact-pack evidence returned by the codegraph oracle.

use serde::{Deserialize, Serialize};

use crate::freshness::SourceFreshness;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CodegraphDashboard {
    pub items: Vec<CodegraphEvidenceItem>,
    #[serde(default)]
    pub tool_build_opportunities: Vec<ToolBuildOpportunityItem>,
    pub freshness: Option<SourceFreshness>,
    pub summary: Option<CodegraphSummary>,
}

impl CodegraphDashboard {
    pub fn misses(&self) -> u32 {
        self.items.iter().filter(|item| item.miss.is_some()).count() as u32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodegraphEvidenceItem {
    pub query_id: String,
    pub tool: String,
    pub repo_id: String,
    pub symbol: String,
    pub schema_version: u32,
    pub references: u32,
    pub reverse_deps: u32,
    pub required_reads: Vec<String>,
    pub proof_lanes: Vec<String>,
    pub suggested_commands: Vec<String>,
    pub miss: Option<String>,
}

impl CodegraphEvidenceItem {
    pub fn new(query_id: impl Into<String>, tool: impl Into<String>) -> Self {
        Self {
            query_id: query_id.into(),
            tool: tool.into(),
            repo_id: String::new(),
            symbol: String::new(),
            schema_version: 2,
            references: 0,
            reverse_deps: 0,
            required_reads: Vec::new(),
            proof_lanes: Vec::new(),
            suggested_commands: Vec::new(),
            miss: None,
        }
    }
}

impl Default for CodegraphEvidenceItem {
    fn default() -> Self {
        Self::new("unknown", "codegraph.query")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CodegraphSummary {
    pub schema_version: u32,
    pub indexed_symbols: u32,
    pub indexed_references: u32,
    pub oracle_queries: u32,
    pub miss_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolBuildOpportunityItem {
    pub cluster_id: String,
    pub repo_id: String,
    pub score: u64,
    pub occurrences: usize,
    pub file_count: usize,
    pub language: String,
    pub suggested_proof_lane: String,
}

impl ToolBuildOpportunityItem {
    pub fn new(cluster_id: impl Into<String>, repo_id: impl Into<String>) -> Self {
        Self {
            cluster_id: cluster_id.into(),
            repo_id: repo_id.into(),
            score: 0,
            occurrences: 0,
            file_count: 0,
            language: String::new(),
            suggested_proof_lane: String::new(),
        }
    }
}

impl Default for ToolBuildOpportunityItem {
    fn default() -> Self {
        Self::new("unknown", "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_default_is_empty() {
        let d = CodegraphDashboard::default();
        assert!(d.items.is_empty());
        assert_eq!(d.misses(), 0);
    }

    #[test]
    fn dashboard_counts_typed_misses() {
        let mut hit = CodegraphEvidenceItem::new("q-1", "code.references");
        hit.symbol = "AgentRunStore".into();
        let mut miss = CodegraphEvidenceItem::new("q-2", "code.definition");
        miss.miss = Some("symbol_not_found".into());
        let d = CodegraphDashboard {
            items: vec![hit, miss],
            tool_build_opportunities: Vec::new(),
            freshness: None,
            summary: None,
        };
        assert_eq!(d.misses(), 1);
    }

    #[test]
    fn dashboard_keeps_tool_build_opportunities_explicit() {
        let mut opportunity = ToolBuildOpportunityItem::new("toolbuild-1", "core/api");
        opportunity.score = 91;
        opportunity.occurrences = 5;
        opportunity.file_count = 3;
        opportunity.language = "rust".into();
        opportunity.suggested_proof_lane = "bash ops/ci/codegraph-tool-build.sh".into();
        let d = CodegraphDashboard {
            items: Vec::new(),
            tool_build_opportunities: vec![opportunity],
            freshness: None,
            summary: None,
        };
        assert_eq!(d.tool_build_opportunities[0].cluster_id, "toolbuild-1");
        assert_eq!(d.tool_build_opportunities[0].score, 91);
    }
}
