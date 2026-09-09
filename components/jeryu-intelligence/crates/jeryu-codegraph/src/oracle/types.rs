use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{epoch_millis, normalize_changed_paths};

/// Repository identity attached to a codegraph query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeGraphRepoIdentity {
    /// Stable Jeryu repository id or `owner/name` fallback.
    pub id: String,
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub name: String,
}

impl CodeGraphRepoIdentity {
    /// Build an identity from an `owner/name` string or opaque repo id.
    #[must_use]
    pub fn from_repo_string(value: &str) -> Self {
        if let Some((owner, name)) = value.split_once('/') {
            return Self {
                id: value.to_string(),
                owner: owner.to_string(),
                name: name.to_string(),
            };
        }
        Self {
            id: value.to_string(),
            owner: "local".to_string(),
            name: value.to_string(),
        }
    }

    /// Local CLI identity derived from a workspace root.
    #[must_use]
    pub fn local(root: &Path) -> Self {
        let name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace")
            .to_string();
        Self {
            id: format!("local/{name}"),
            owner: "local".to_string(),
            name,
        }
    }
}

/// Query body shared by REST and CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeGraphQuery {
    /// Requested ref name. Serialized as `ref` for the public contract.
    #[serde(default = "default_ref_name", rename = "ref")]
    pub ref_name: String,
    /// Repo-relative changed paths.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// Optional short task intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Optional code question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Optional output token target for downstream callers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl Default for CodeGraphQuery {
    fn default() -> Self {
        Self {
            ref_name: default_ref_name(),
            changed_paths: Vec::new(),
            intent: None,
            question: None,
            max_tokens: Some(12_000),
        }
    }
}

/// Default ref for public requests.
#[must_use]
pub fn default_ref_name() -> String {
    "main".to_string()
}

/// MCP query input. The `repo` identity is required because MCP has no path
/// parameter equivalent to the REST route.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeGraphMcpQuery {
    /// Opaque repo id or `owner/name`.
    pub repo: String,
    /// Shared query body.
    #[serde(flatten)]
    pub query: CodeGraphQuery,
}

/// One provenance record on a response item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeGraphProvenance {
    /// Source subsystem or reason.
    pub source: String,
    /// Human-readable detail.
    pub detail: String,
    /// Source path when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Generated-zone metadata attached to a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedZoneHit {
    /// Zone path or glob.
    pub path: String,
    /// Generator command from governance metadata.
    pub generator: String,
    /// Whether manual edits are permitted.
    pub manual_edits: bool,
}

/// A file the downstream agent should read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeContextFile {
    /// Repo-relative path.
    pub path: String,
    /// Ordered inclusion reasons.
    pub reasons: Vec<String>,
    /// Lower ranks are more important.
    pub rank: u32,
    /// Owner-map owner when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Test-map or proof-lane ids attached to this file.
    pub proof_lanes: Vec<String>,
    /// Generated-zone match when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_zone: Option<GeneratedZoneHit>,
    /// Whether the file should be treated as directly editable.
    pub editable: bool,
    /// Provenance for why this file is included.
    pub provenance: Vec<CodeGraphProvenance>,
}

/// Public symbol affected by the query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolImpact {
    /// Owning crate.
    pub crate_name: String,
    /// Symbol name.
    pub symbol: String,
    /// Symbol kind.
    pub kind: String,
    /// Repo-relative source file.
    pub file: String,
    /// Provenance for the symbol.
    pub provenance: Vec<CodeGraphProvenance>,
}

/// Proof lane attached to impacted paths.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofLaneImpact {
    /// Lane id.
    pub lane: String,
    /// Required commands.
    pub required_commands: Vec<String>,
    /// Whether the lane blocks merge.
    pub blocks_merge: bool,
    /// Why this lane was selected.
    pub reason: String,
    /// Provenance for the lane.
    pub provenance: Vec<CodeGraphProvenance>,
}

/// A heuristic-only file match excluded from authoritative context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExcludedFile {
    /// Repo-relative path.
    pub path: String,
    /// Exclusion reason.
    pub reason: String,
    /// Provenance for the exclusion.
    pub provenance: Vec<CodeGraphProvenance>,
}

/// Graph counts and enabled analyzer scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphStats {
    /// Indexed public symbols.
    pub symbol_count: usize,
    /// Workspace-internal crate dependency edges.
    pub crate_dep_edges: usize,
    /// Indexed file rows.
    pub indexed_file_count: usize,
    /// Loaded governance files.
    pub governance_file_count: usize,
    /// Enabled authoritative analyzers.
    pub analyzers: Vec<String>,
}

/// Receipt for the index refresh that produced a pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexReceipt {
    /// Stable run id.
    pub run_id: String,
    /// Store path that received the refreshed index.
    pub store_path: String,
    /// Ref indexed.
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// Commit indexed.
    pub commit: String,
    /// Timestamp for the refresh.
    pub indexed_at: String,
    /// Enabled authoritative analyzers.
    pub analyzer_scope: Vec<String>,
}

/// Full codegraph impact pack returned by REST, MCP, and CLI JSON output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeGraphImpactPack {
    /// Compatibility schema discriminator.
    pub schema_version: String,
    /// Repository identity.
    pub repo: CodeGraphRepoIdentity,
    /// Ref requested.
    #[serde(rename = "ref")]
    pub ref_name: String,
    /// Resolved commit.
    pub commit: String,
    /// Normalized changed paths.
    pub changed_paths: Vec<String>,
    /// Optional task intent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Optional code question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Requested token budget.
    pub max_tokens: u32,
    /// Crates owning changed paths.
    pub changed_crates: Vec<String>,
    /// Crates affected by reverse dependency reachability.
    pub affected_crates: Vec<String>,
    /// Affected public symbols.
    pub affected_symbols: Vec<SymbolImpact>,
    /// Authoritative read context.
    pub must_read_files: Vec<CodeContextFile>,
    /// Governance and affected-context read suggestions.
    pub should_read_files: Vec<CodeContextFile>,
    /// Proof lanes selected from governance metadata.
    pub proof_lanes: Vec<ProofLaneImpact>,
    /// Suggested local commands.
    pub suggested_commands: Vec<String>,
    /// Heuristic-only matches excluded from authoritative context.
    pub excluded_files: Vec<ExcludedFile>,
    /// Graph counts.
    pub graph_stats: GraphStats,
    /// Honest residual risk notes.
    pub residual_risk: Vec<String>,
    /// Pack-level provenance.
    pub provenance: Vec<CodeGraphProvenance>,
    /// Index receipt.
    pub index_receipt: IndexReceipt,
}

impl CodeGraphImpactPack {
    /// Build an empty but schema-complete pack.
    #[must_use]
    pub fn empty_contract(
        repo: CodeGraphRepoIdentity,
        ref_name: String,
        commit: String,
        query: CodeGraphQuery,
    ) -> Self {
        let analyzer_scope = vec!["rust_cargo_exact".to_string()];
        Self {
            schema_version: "codegraph.query/v1".to_string(),
            repo,
            ref_name: ref_name.clone(),
            commit: commit.clone(),
            changed_paths: normalize_changed_paths(&query.changed_paths),
            intent: query.intent,
            question: query.question,
            max_tokens: query.max_tokens.unwrap_or(12_000),
            changed_crates: Vec::new(),
            affected_crates: Vec::new(),
            affected_symbols: Vec::new(),
            must_read_files: Vec::new(),
            should_read_files: Vec::new(),
            proof_lanes: Vec::new(),
            suggested_commands: Vec::new(),
            excluded_files: Vec::new(),
            graph_stats: GraphStats {
                symbol_count: 0,
                crate_dep_edges: 0,
                indexed_file_count: 0,
                governance_file_count: 0,
                analyzers: analyzer_scope.clone(),
            },
            residual_risk: vec![
                "memory backend did not materialize a hosted repository".to_string(),
                "typescript/vite/react/security analyzers are outside the v1 authoritative analyzer scope".to_string(),
            ],
            provenance: vec![CodeGraphProvenance {
                source: "mcp_memory_backend".to_string(),
                detail: "schema-complete deterministic contract".to_string(),
                path: None,
            }],
            index_receipt: IndexReceipt {
                run_id: "memory-contract".to_string(),
                store_path: "memory".to_string(),
                ref_name,
                commit,
                indexed_at: epoch_millis().to_string(),
                analyzer_scope,
            },
        }
    }
}
