//! Tool-adoption fleet contracts: a projection of every repo's latest recorded
//! jankurai score (`tool_adoption.items`) into a per-tool matrix of who adopts
//! each tool and who is applicable-but-missing. Powers the SPA Tool Fleet page —
//! the visibility surface for tool compounding ("which repos use which tools,
//! which should").

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One tool's adoption across the fleet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFleetEntry {
    /// jankurai tool id (e.g. `audit-ci`, `security`).
    pub tool: String,
    /// Tool category (`audit`, `proof`, `security`, …).
    pub category: String,
    /// Repos (`owner/name`) where the tool is adopted (configured or better).
    pub adopting_repos: Vec<String>,
    /// Repos where the tool is applicable but NOT adopted — the "should adopt"
    /// list that quantifies the remaining tool-compounding opportunity.
    pub applicable_missing_repos: Vec<String>,
}

/// Fleet-wide jankurai tool-adoption matrix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFleetResponse {
    /// Repos with at least one recorded score carrying a tool-adoption block.
    pub repos_scored: u32,
    /// Per-tool adoption, sorted by tool id.
    pub tools: Vec<ToolFleetEntry>,
}
