use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CodeGraphError, Result};

use super::schema::{DEFAULT_NAME, DEFAULT_OWNER, DEFAULT_REPO_ID};

/// A repository identity stored alongside graph rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoIdentity {
    /// Stable repository identifier.
    pub repo_id: String,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub name: String,
}

impl Default for RepoIdentity {
    fn default() -> Self {
        Self {
            repo_id: DEFAULT_REPO_ID.to_string(),
            owner: DEFAULT_OWNER.to_string(),
            name: DEFAULT_NAME.to_string(),
        }
    }
}

/// Query options that participate in cache keying.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryOptions {
    /// Maximum token budget for the query.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Optional proof lanes included in the analyzer scope.
    #[serde(default)]
    pub proof_lanes: Vec<String>,
}

impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            proof_lanes: Vec::new(),
        }
    }
}

impl QueryOptions {
    /// Validates the query options.
    pub fn validate(&self) -> Result<()> {
        if self.max_tokens == 0 {
            return Err(CodeGraphError::InvalidMaxTokens {
                value: self.max_tokens,
                reason: "max_tokens must be greater than zero".to_string(),
            });
        }
        if self.max_tokens > 60_000 {
            return Err(CodeGraphError::InvalidMaxTokens {
                value: self.max_tokens,
                reason: "max_tokens must not exceed 60000".to_string(),
            });
        }
        Ok(())
    }

    /// Serializes the options with a stable compact JSON representation.
    pub fn canonical_json(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|e| CodeGraphError::Storage(e.to_string()))
    }
}

/// The storage backend used for receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheStatus {
    /// The query reused a cached index.
    Hit,
    /// The query refreshed the index and wrote a new run.
    Refreshed,
}

/// Summary stats for a stored graph snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStats {
    /// Number of symbol rows.
    pub symbols: usize,
    /// Number of crate dependency rows.
    pub crate_deps: usize,
}

/// A receipt for an index run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexReceipt {
    /// Unique run identifier.
    pub run_id: String,
    /// Optional outbox event identifier for refreshes.
    pub outbox_event_id: Option<String>,
    /// Storage backend name.
    pub storage_backend: String,
    /// Repository identity.
    pub repo: RepoIdentity,
    /// Ref name that was indexed.
    pub ref_name: String,
    /// Commit SHA that was indexed.
    pub commit_sha: String,
    /// Workspace root that was indexed.
    pub root: String,
    /// UTC timestamp when the run was recorded.
    pub indexed_at: String,
    /// Analyzer scope JSON.
    pub analyzer_scope_json: String,
    /// Graph stats JSON.
    pub graph_stats_json: String,
    /// Schema version used for the run.
    pub schema_version: i64,
    /// Schema digest used for the run.
    pub schema_digest: String,
    /// Cache result for the current query.
    pub cache_status: CacheStatus,
}

/// A row in `codegraph_symbols`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRow {
    /// Owning crate (workspace package name).
    pub crate_name: String,
    /// Repo-relative source file path.
    pub file: String,
    /// Symbol name.
    pub symbol: String,
    /// Symbol kind (e.g. `public`).
    pub kind: String,
    /// Whether the symbol is part of the public API.
    pub is_public: bool,
    /// 1-based line number (0 when unknown).
    pub line: u32,
}

/// A row in `codegraph_crate_deps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateDepRow {
    /// Dependent crate.
    pub crate_name: String,
    /// Crate it depends on.
    pub depends_on: String,
}

/// A persistable snapshot of the code graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphSnapshot {
    /// All indexed symbol rows.
    pub symbols: Vec<SymbolRow>,
    /// All recorded crate dependency edges.
    pub crate_deps: Vec<CrateDepRow>,
}

/// File-level code graph metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// Repository identity.
    pub repo: RepoIdentity,
    /// Commit SHA.
    pub commit_sha: String,
    /// Repo-relative file path.
    pub path: String,
    /// Owning crate.
    pub crate_name: String,
    /// Language tag.
    pub language: String,
    /// Owner tag.
    pub owner: String,
    /// Test lane tag.
    pub test_lane: String,
    /// Proof lanes associated with the file.
    pub proof_lanes: Vec<String>,
    /// Whether the file is in a generated zone.
    pub generated_zone: bool,
    /// Whether the file is editable.
    pub editable: bool,
    /// Provenance payload.
    pub provenance: Value,
}

/// Governance metadata row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRecord {
    /// Repository identity.
    pub repo: RepoIdentity,
    /// Commit SHA.
    pub commit_sha: String,
    /// Repo-relative path.
    pub path: String,
    /// Governance kind.
    pub kind: String,
    /// Governance digest.
    pub digest: String,
    /// Whether the row has been loaded.
    pub loaded: bool,
}

fn default_max_tokens() -> u32 {
    12_000
}
