use serde::{Deserialize, Serialize};

/// A row in `codegraph_symbols`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrateDepRow {
    /// Dependent crate.
    pub crate_name: String,
    /// Crate it depends on.
    pub depends_on: String,
}

/// A row in `codegraph_symbol_refs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolRefRow {
    /// Owning crate for the referenced symbol.
    pub crate_name: String,
    /// Definition file for the referenced symbol.
    pub file: String,
    /// Referenced symbol name.
    pub symbol: String,
    /// Repo-relative file containing the reference.
    pub ref_file: String,
    /// 1-based reference line number (0 when unknown).
    pub ref_line: u32,
    /// Reference kind, for example `call`, `type`, or `mention`.
    pub ref_kind: String,
}

/// A repo file recorded with governance and provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRow {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit sha the row was indexed from.
    pub commit_sha: String,
    /// Repo-relative file path.
    pub path: String,
    /// Owning Rust crate when known.
    pub crate_name: Option<String>,
    /// Analyzer language/domain label.
    pub language: String,
    /// Owner-map owner when known.
    pub owner: Option<String>,
    /// Test-map lane when known.
    pub test_lane: Option<String>,
    /// Proof lanes attached to the file.
    pub proof_lanes: Vec<String>,
    /// Matching generated-zone path when the file is generated.
    pub generated_zone: Option<String>,
    /// Whether an agent should treat the file as directly editable.
    pub editable: bool,
    /// JSON provenance records for this row.
    pub provenance_json: String,
}

/// A loaded governance metadata file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRow {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit sha the row was loaded from.
    pub commit_sha: String,
    /// Repo-relative governance path.
    pub path: String,
    /// Governance kind, such as `owner_map` or `proof_lanes`.
    pub kind: String,
    /// Lightweight content digest for provenance.
    pub digest: String,
    /// Whether the file was present and loaded.
    pub loaded: bool,
}

/// Receipt for one index refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRunRow {
    /// Stable run id for this refresh.
    pub run_id: String,
    /// Stable repository id.
    pub repo_id: String,
    /// Ref name requested by the caller.
    pub ref_name: String,
    /// Commit sha indexed.
    pub commit_sha: String,
    /// Materialized root that was indexed.
    pub root: String,
    /// Timestamp for the refresh.
    pub indexed_at: String,
    /// JSON array of enabled analyzers.
    pub analyzer_scope_json: String,
    /// JSON object with graph counts.
    pub graph_stats_json: String,
}

/// A persistable snapshot of the code graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// All indexed symbol rows.
    pub symbols: Vec<SymbolRow>,
    /// All recorded crate dependency edges.
    pub crate_deps: Vec<CrateDepRow>,
    /// All recorded symbol reference rows.
    pub symbol_refs: Vec<SymbolRefRow>,
    /// Files with attached governance metadata.
    pub files: Vec<FileRow>,
    /// Governance metadata files loaded for this snapshot.
    pub governance: Vec<GovernanceRow>,
    /// Index refresh receipts.
    pub index_runs: Vec<IndexRunRow>,
}
