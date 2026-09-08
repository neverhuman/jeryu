//! Tool-finder contracts: the system-wide duplicate-code dashboard behind
//! `/tools` and the live scan it drives. The discovery engine lives in
//! `jeryu-codegraph` (cross-repo window clusters, overlap merging, pattern
//! families); these DTOs are the read surface the SPA renders plus the scan
//! status streamed over the `tool_finder.scan` WebSocket scope.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Live (or last) system-wide scan status. Snapshot on subscribe, then pushed
/// on every throttled progress event over scope `tool_finder.scan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderScanStatus {
    /// Whether a scan is currently in flight.
    pub running: bool,
    /// Monotonic per-process scan counter (0 = never run this process).
    pub scan_id: u32,
    /// `idle` | `discover` | `scan` | `merge` | `families` | `finalize` |
    /// `completed` | `failed`.
    pub phase: String,
    /// Repo currently being fingerprinted (scan phase only).
    pub current_repo: Option<String>,
    /// Total repos in the scan.
    pub repos_total: u32,
    /// Repos fully scanned so far.
    pub repos_done: u32,
    /// Files fingerprinted so far across all repos.
    pub files_scanned: u32,
    /// Files skipped (size, decode, exclusion, corpus-scale guard).
    pub files_skipped: u32,
    /// Ranked clusters found (populated near the end of the pipeline).
    pub clusters_found: u32,
    /// Pattern families grouped from the clusters.
    pub families_found: u32,
    /// RFC3339 start time of the running/last scan.
    pub started_at: Option<String>,
    /// RFC3339 finish time of the last completed/failed scan.
    pub finished_at: Option<String>,
    /// Failure detail when `phase` is `failed`.
    pub error: Option<String>,
}

/// One code occurrence inside a cluster, pointing at an exact repo file span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderOccurrence {
    /// Repo the occurrence lives in.
    pub repo_id: String,
    /// Repo-relative path.
    pub path: String,
    /// 1-based start line.
    pub start_line: u32,
    /// 1-based end line.
    pub end_line: u32,
    /// Whether the occurrence lives under a test path.
    pub is_test: bool,
}

/// One ranked duplicate-code cluster, enriched with proposal metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderCluster {
    /// Stable fingerprint-derived id (`toolbuild-…`).
    pub cluster_id: String,
    /// `tool-candidate` | `managed-scaffold` | `config-pattern` | `test-pattern`.
    pub category: String,
    /// Heuristic ranking score.
    pub score: u64,
    /// Occurrences across all repos.
    pub occurrence_count: u32,
    /// Distinct repos the cluster spans.
    pub repo_count: u32,
    /// Distinct files the cluster spans.
    pub file_count: u32,
    /// Total duplicated source lines across occurrences.
    pub total_lines: u32,
    /// Dominant language label.
    pub language: String,
    /// Deterministic one-line description.
    pub insight: String,
    /// Identifier/literal-normalized window preview.
    pub normalized_preview: String,
    /// Lines saved if all occurrences collapse into one shared tool.
    pub anticipated_loc_saved: u32,
    /// Suggested registry tool name for a proposal.
    pub suggested_name: String,
    /// Suggested registry tool kind for a proposal.
    pub suggested_kind: String,
    /// Whether durable ignore feedback suppresses this cluster.
    pub ignored: bool,
    /// Representative occurrences (capped; every spanning repo represented).
    pub occurrences: Vec<ToolFinderOccurrence>,
}

/// A pattern family: clusters whose anchor signatures overlap — one repeated
/// pattern in several close variants. The dashboard's top-level card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderPatternFamily {
    /// Stable id derived from (language, category, union anchor signature).
    pub family_id: String,
    /// Human label mined from the most frequent shared anchors.
    pub label: String,
    /// Category shared by every member cluster.
    pub category: String,
    /// Dominant language.
    pub language: String,
    /// Sum of the members' anticipated LOC saved.
    pub anticipated_loc_saved: u32,
    /// Total occurrences across member clusters.
    pub occurrence_count: u32,
    /// Total distinct files across member clusters.
    pub file_count: u32,
    /// Repos the family spans.
    pub repos: Vec<String>,
    /// Member clusters, ranked by score.
    pub clusters: Vec<ToolFinderCluster>,
}

/// Metadata describing the persisted scan the dashboard is built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderScanMeta {
    /// Unix-millis timestamp of the persisted scan (None = never scanned).
    pub scanned_at: Option<String>,
    /// Repos covered by the persisted scan.
    pub repos_scanned: u32,
    /// Files fingerprinted by the persisted scan.
    pub files_scanned: u32,
}

/// The `/tools` dashboard: pattern families over the persisted system scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderDashboard {
    /// RFC3339 timestamp the dashboard was computed.
    pub generated_at: String,
    /// Persisted-scan provenance.
    pub scan: ToolFinderScanMeta,
    /// Number of pattern families.
    pub family_count: u32,
    /// Number of ranked clusters across all families.
    pub cluster_count: u32,
    /// Sum of anticipated LOC saved across tool-candidate families.
    pub candidate_loc_saved: u32,
    /// Families, sorted by anticipated LOC saved.
    pub families: Vec<ToolFinderPatternFamily>,
}

/// Receipt for promoting a cluster into a `jeryu-tool` registry proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolFinderProposeReceipt {
    /// Whether a new proposal was filed (false = already proposed, no-op).
    pub created: bool,
    /// Registry tool id the cluster maps to.
    pub tool_id: String,
    /// Build-task id filed alongside a new proposal.
    pub task_id: Option<String>,
    /// Human-readable outcome.
    pub message: String,
}
