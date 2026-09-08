//! Reusable-tool registry contracts: a projection of `jeryu-tool`'s
//! `tools-registry.toml` (shared crates / TS / React / shell libraries that
//! replace copy-pasted code across repos) into the "golden box" summary the
//! repos page renders. The discovery side lives in `jeryu-tool-finder`; this is
//! the *adoption + payoff* surface (how many tools, who adopts them, LOC saved).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One reusable tool's headline numbers for the golden box.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolRegistryEntry {
    /// Stable registry id (e.g. `jeryu-ci-shell-lib`).
    pub id: String,
    /// Human label.
    pub name: String,
    /// `rust-crate` | `ts-lib` | `react-component` | `vite-plugin` | `shell-lib`.
    pub kind: String,
    /// `proposed` | `building` | `published` | `deprecated`.
    pub status: String,
    /// Number of repos already using the tool.
    pub adopting_repo_count: u32,
    /// Number of repos that still carry a copy and should adopt.
    pub candidate_repo_count: u32,
    /// Realized lines removed, summed across adopters.
    pub loc_saved: u32,
    /// Anticipated lines still to be removed once candidates migrate.
    pub loc_saved_estimate: u32,
}

/// Family-wide reusable-tool registry summary. Powers the gold "tool control
/// plane" box on the repos page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolRegistrySummary {
    /// RFC3339 timestamp the summary was computed.
    pub generated_at: String,
    /// Total registered tools.
    pub tool_count: u32,
    /// Tools with status `published`.
    pub published_count: u32,
    /// Tools with status `building`.
    pub building_count: u32,
    /// Tools with status `proposed`.
    pub proposed_count: u32,
    /// Tools with status `deprecated`.
    pub deprecated_count: u32,
    /// Distinct repos adopting at least one tool.
    pub adopting_repo_count: u32,
    /// Distinct repos that are a candidate for at least one tool.
    pub candidate_repo_count: u32,
    /// Open (or in-progress) build tasks awaiting a tool.
    pub open_task_count: u32,
    /// Realized LOC saved across all tools.
    pub realized_loc_saved: u32,
    /// Anticipated LOC saved across all tools.
    pub anticipated_loc_saved: u32,
    /// Per-tool rows, sorted by realized then anticipated LOC saved.
    pub tools: Vec<ToolRegistryEntry>,
}
