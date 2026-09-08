//! Progress events for live-streaming a multi-repo tool-build scan.

use serde::{Deserialize, Serialize};

/// Scan pipeline phase, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolBuildScanPhase {
    /// Enumerating repos and their files.
    Discover,
    /// Fingerprinting windows repo by repo.
    Scan,
    /// Folding worker shards and chaining overlapping windows.
    Merge,
    /// Grouping clusters into pattern families.
    Families,
    /// Ranking, capping, and building the report.
    Finalize,
}

impl ToolBuildScanPhase {
    /// Stable kebab-case label for transport layers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discover => "discover",
            Self::Scan => "scan",
            Self::Merge => "merge",
            Self::Families => "families",
            Self::Finalize => "finalize",
        }
    }
}

/// One progress observation. Counters are cumulative across the whole scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanProgress {
    /// Current pipeline phase.
    pub phase: ToolBuildScanPhase,
    /// Index of the repo this event concerns (0-based; meaningful in Scan).
    pub repo_index: usize,
    /// Total repos in the scan.
    pub repo_total: usize,
    /// Repos fully scanned so far.
    pub repos_done: usize,
    /// Repo id this event concerns (empty outside Scan).
    pub current_repo: String,
    /// Files fingerprinted so far across all repos.
    pub files_scanned: usize,
    /// Files skipped so far (size, decode, or exclusion).
    pub files_skipped: usize,
    /// Ranked clusters so far (0 until Merge completes).
    pub clusters_so_far: usize,
}
