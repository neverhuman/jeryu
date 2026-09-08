//! Codegraph oracle services and compatibility facade.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use jeryu_rustjet::WorkspaceGraph;
use serde::{Deserialize, Serialize};

use governance::GovernanceMetadata;

use crate::graph::CodeGraph;
use crate::graph::ImpactReport;
use crate::storage::{
    CodeGraphStore, FileRow, GovernanceRow, GraphSnapshot, IndexRunRow, SymbolRefRow, SymbolRow,
};
use crate::{Result, error::CodeGraphError};

mod governance;
mod types;

pub use types::{
    CodeContextFile, CodeGraphImpactPack, CodeGraphMcpQuery, CodeGraphProvenance, CodeGraphQuery,
    CodeGraphRepoIdentity, ExcludedFile, GeneratedZoneHit, GraphStats, IndexReceipt,
    ProofLaneImpact, SymbolImpact, default_ref_name,
};

/// Query service for a materialized repository root and SQLite store.
#[derive(Debug, Clone)]
pub struct CodeGraphService {
    root: PathBuf,
    store: CodeGraphStore,
}

impl CodeGraphService {
    /// Build a service from a materialized repository root and store.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, store: CodeGraphStore) -> Self {
        Self {
            root: root.into(),
            store,
        }
    }

    /// Build/refresh the graph and return an auditable impact pack.
    pub fn query(
        &self,
        repo: CodeGraphRepoIdentity,
        commit: impl Into<String>,
        mut query: CodeGraphQuery,
    ) -> Result<CodeGraphImpactPack> {
        query.changed_paths = normalize_changed_paths(&query.changed_paths);
        let commit = commit.into();
        let workspace = WorkspaceGraph::load(&self.root)
            .map_err(|e| CodeGraphError::Workspace(e.to_string()))?;
        let graph = CodeGraph::index_workspace(&workspace)?;
        let impact = graph.impact_of(&workspace, &query.changed_paths);
        let governance = GovernanceMetadata::load(&self.root)?;
        let analyzer_scope = vec!["rust_cargo_exact".to_string()];
        let indexed_at = epoch_millis().to_string();
        let run_id = format!("codegraph-{}-{}", sanitize_id(&repo.id), indexed_at);

        let mut snapshot = graph.snapshot().clone();
        attach_governance_rows(&mut snapshot, &governance, &repo, &commit);

        let graph_stats = GraphStats {
            symbol_count: snapshot.symbols.len(),
            crate_dep_edges: snapshot.crate_deps.len(),
            indexed_file_count: snapshot.files.len(),
            governance_file_count: governance.loaded_files.len(),
            analyzers: analyzer_scope.clone(),
        };
        snapshot.index_runs.push(IndexRunRow {
            run_id: run_id.clone(),
            repo_id: repo.id.clone(),
            ref_name: query.ref_name.clone(),
            commit_sha: commit.clone(),
            root: self.root.display().to_string(),
            indexed_at: indexed_at.clone(),
            analyzer_scope_json: serde_json::to_string(&analyzer_scope)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
            graph_stats_json: serde_json::to_string(&graph_stats)
                .map_err(|e| CodeGraphError::Storage(e.to_string()))?,
        });
        self.store.persist(&snapshot)?;

        Ok(build_pack(
            repo,
            commit,
            query,
            PackBuildInput {
                workspace: &workspace,
                snapshot: &snapshot,
                impact: &impact,
                governance: &governance,
                graph_stats,
                index_receipt: IndexReceipt {
                    run_id,
                    store_path: self.store.path().display().to_string(),
                    ref_name: match snapshot.index_runs.last() {
                        Some(row) => row.ref_name.clone(),
                        None => String::new(),
                    },
                    commit: match snapshot.index_runs.last() {
                        Some(row) => row.commit_sha.clone(),
                        None => String::new(),
                    },
                    indexed_at,
                    analyzer_scope,
                },
            },
        ))
    }
}

struct PackBuildInput<'a> {
    workspace: &'a WorkspaceGraph,
    snapshot: &'a GraphSnapshot,
    impact: &'a ImpactReport,
    governance: &'a GovernanceMetadata,
    graph_stats: GraphStats,
    index_receipt: IndexReceipt,
}

fn build_pack(
    repo: CodeGraphRepoIdentity,
    commit: String,
    query: CodeGraphQuery,
    input: PackBuildInput<'_>,
) -> CodeGraphImpactPack {
    let PackBuildInput {
        workspace,
        snapshot,
        impact,
        governance,
        graph_stats,
        index_receipt,
    } = input;
    let mut must = BTreeMap::new();
    let mut should = BTreeMap::new();
    let file_rows: BTreeMap<&str, &FileRow> = snapshot
        .files
        .iter()
        .map(|row| (row.path.as_str(), row))
        .collect();

    for path in &query.changed_paths {
        insert_context(
            &mut must,
            path,
            0,
            "input_changed_path",
            "changed path supplied by caller",
            governance,
            file_rows.get(path.as_str()).copied(),
        );
    }

    for crate_name in &impact.changed_crates {
        if let Some(package) = workspace.package(crate_name) {
            let path = manifest_relative_path(package.relative_root.as_str());
            insert_context(
                &mut must,
                &path,
                20,
                "changed_crate_manifest",
                "Cargo manifest for a changed crate",
                governance,
                file_rows.get(path.as_str()).copied(),
            );
        }
    }

    for symbol in &snapshot.symbols {
        if impact.changed_crates.contains(&symbol.crate_name) {
            insert_context(
                &mut must,
                &symbol.file,
                30,
                "changed_crate_public_symbol_file",
                "public symbol file in a changed crate",
                governance,
                file_rows.get(symbol.file.as_str()).copied(),
            );
        } else if impact.affected_crates.contains(&symbol.crate_name) {
            insert_context(
                &mut should,
                &symbol.file,
                120,
                "affected_crate_public_symbol_file",
                "public symbol file in a reverse-dependent crate",
                governance,
                file_rows.get(symbol.file.as_str()).copied(),
            );
        }
    }

    for crate_name in impact.affected_crates.difference(&impact.changed_crates) {
        if let Some(package) = workspace.package(crate_name) {
            let path = manifest_relative_path(package.relative_root.as_str());
            insert_context(
                &mut should,
                &path,
                110,
                "affected_crate_manifest",
                "Cargo manifest for a reverse-dependent crate",
                governance,
                file_rows.get(path.as_str()).copied(),
            );
        }
    }

    for loaded in &governance.loaded_files {
        if !must.contains_key(loaded.path.as_str()) {
            insert_context(
                &mut should,
                &loaded.path,
                200,
                "governance_metadata",
                "loaded Jankurai governance metadata",
                governance,
                file_rows.get(loaded.path.as_str()).copied(),
            );
        }
    }

    let must_paths: BTreeSet<_> = must.keys().cloned().collect();
    should.retain(|path, _| !must_paths.contains(path));

    let mut proof_lanes = selected_proof_lanes(&query.changed_paths, governance);
    let mut suggested_commands: BTreeSet<String> = BTreeSet::new();
    for path in &query.changed_paths {
        if let Some(rule) = governance.test_for_path(path) {
            suggested_commands.insert(rule.command.clone());
        }
    }
    for lane in &proof_lanes {
        for command in &lane.required_commands {
            suggested_commands.insert(command.clone());
        }
    }
    for crate_name in &impact.changed_crates {
        suggested_commands.insert(format!("cargo test -p {crate_name} --jobs 40"));
    }
    proof_lanes.sort_by(|a, b| a.lane.cmp(&b.lane));

    let excluded_files = lexical_exclusions(&query, &must, &should, governance);

    let mut residual_risk = vec![
        "typescript/vite/react/security analyzers are outside the v1 authoritative analyzer scope; no authoritative results are emitted for those domains".to_string(),
    ];
    let unmapped: Vec<_> = query
        .changed_paths
        .iter()
        .filter(|path| workspace.package_for_path(path).is_none())
        .cloned()
        .collect();
    if !unmapped.is_empty() {
        residual_risk.push(format!(
            "changed paths not owned by a Rust workspace crate: {}",
            unmapped.join(", ")
        ));
    }
    if !excluded_files.is_empty() {
        residual_risk.push(
            "heuristic-only lexical matches were excluded from must_read context".to_string(),
        );
    }

    CodeGraphImpactPack {
        schema_version: "codegraph.query/v1".to_string(),
        repo,
        ref_name: query.ref_name.clone(),
        commit,
        changed_paths: query.changed_paths.clone(),
        intent: query.intent.clone(),
        question: query.question.clone(),
        max_tokens: query.max_tokens.unwrap_or(12_000),
        changed_crates: impact.changed_crates.iter().cloned().collect(),
        affected_crates: impact.affected_crates.iter().cloned().collect(),
        affected_symbols: affected_symbols(&snapshot.symbols, &impact.affected_crates),
        must_read_files: sorted_context(must),
        should_read_files: sorted_context(should),
        proof_lanes,
        suggested_commands: suggested_commands.into_iter().collect(),
        excluded_files,
        graph_stats,
        residual_risk,
        provenance: vec![
            CodeGraphProvenance {
                source: "git_ref".to_string(),
                detail: "repo/ref resolved before materialized indexing".to_string(),
                path: None,
            },
            CodeGraphProvenance {
                source: "rust_cargo_exact".to_string(),
                detail: "WorkspaceGraph package and reverse-dependency reachability".to_string(),
                path: Some("Cargo.toml".to_string()),
            },
            CodeGraphProvenance {
                source: "governance_ingestion".to_string(),
                detail: "Jankurai governance files loaded when present".to_string(),
                path: None,
            },
            CodeGraphProvenance {
                source: "sqlite_index_receipt".to_string(),
                detail: "index refresh persisted to SQLite".to_string(),
                path: Some(index_receipt.store_path.clone()),
            },
        ],
        index_receipt,
    }
}

fn affected_symbols(
    symbols: &[SymbolRow],
    affected_crates: &BTreeSet<String>,
) -> Vec<SymbolImpact> {
    symbols
        .iter()
        .filter(|row| affected_crates.contains(&row.crate_name))
        .map(|row| SymbolImpact {
            crate_name: row.crate_name.clone(),
            symbol: row.symbol.clone(),
            kind: row.kind.clone(),
            file: row.file.clone(),
            provenance: vec![CodeGraphProvenance {
                source: "rust_public_symbol_index".to_string(),
                detail: format!("symbol owned by affected crate {}", row.crate_name),
                path: Some(row.file.clone()),
            }],
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn insert_context(
    files: &mut BTreeMap<String, CodeContextFile>,
    path: &str,
    rank: u32,
    reason: &str,
    detail: &str,
    governance: &GovernanceMetadata,
    row: Option<&FileRow>,
) {
    let owner = match row {
        Some(file) => file.owner.clone(),
        None => governance.owner_for_path(path),
    };
    let mut proof_lanes = match row {
        Some(file) => file.proof_lanes.clone(),
        None => Vec::new(),
    };
    if let Some(rule) = governance.test_for_path(path)
        && !proof_lanes.contains(&rule.lane)
    {
        proof_lanes.push(rule.lane.clone());
    }
    proof_lanes.sort();
    proof_lanes.dedup();
    let generated_zone = governance.generated_zone_for_path(path);
    let editable = match row {
        Some(file) => file.editable,
        None => generated_zone.as_ref().is_none_or(|zone| zone.manual_edits),
    };

    let entry = files
        .entry(path.to_string())
        .or_insert_with(|| CodeContextFile {
            path: path.to_string(),
            reasons: Vec::new(),
            rank,
            owner,
            proof_lanes,
            generated_zone,
            editable,
            provenance: Vec::new(),
        });
    if !entry.reasons.iter().any(|value| value == reason) {
        entry.reasons.push(reason.to_string());
    }
    entry.rank = entry.rank.min(rank);
    entry.provenance.push(CodeGraphProvenance {
        source: reason.to_string(),
        detail: detail.to_string(),
        path: Some(path.to_string()),
    });
}

fn sorted_context(files: BTreeMap<String, CodeContextFile>) -> Vec<CodeContextFile> {
    let mut files: Vec<_> = files.into_values().collect();
    files.sort_by(|a, b| (a.rank, a.path.as_str()).cmp(&(b.rank, b.path.as_str())));
    files
}

fn selected_proof_lanes(
    changed_paths: &[String],
    governance: &GovernanceMetadata,
) -> Vec<ProofLaneImpact> {
    let mut lanes: BTreeMap<String, ProofLaneImpact> = BTreeMap::new();
    for path in changed_paths {
        let Some(test_rule) = governance.test_for_path(path) else {
            continue;
        };
        let lane_rule = governance.proof_lanes.get(&test_rule.lane);
        let required_commands = match lane_rule {
            Some(lane) if !lane.required.is_empty() => lane.required.clone(),
            _ => vec![test_rule.command.clone()],
        };
        lanes
            .entry(test_rule.lane.clone())
            .or_insert(ProofLaneImpact {
                lane: test_rule.lane.clone(),
                required_commands,
                blocks_merge: lane_rule.is_some_and(|lane| lane.blocks_merge),
                reason: format!("changed path {path} maps to test lane {}", test_rule.lane),
                provenance: vec![CodeGraphProvenance {
                    source: "test_map".to_string(),
                    detail: test_rule.purpose.clone(),
                    path: Some(path.to_string()),
                }],
            });
    }
    lanes.into_values().collect()
}

fn lexical_exclusions(
    query: &CodeGraphQuery,
    must: &BTreeMap<String, CodeContextFile>,
    should: &BTreeMap<String, CodeContextFile>,
    governance: &GovernanceMetadata,
) -> Vec<ExcludedFile> {
    let tokens = lexical_tokens(query);
    if tokens.is_empty() {
        return Vec::new();
    }
    let included: BTreeSet<&str> = must
        .keys()
        .chain(should.keys())
        .map(String::as_str)
        .collect();
    let mut excluded = Vec::new();
    for path in &governance.repo_files {
        if included.contains(path.as_str()) {
            continue;
        }
        let lower = path.to_ascii_lowercase();
        if tokens.iter().any(|token| lower.contains(token)) {
            excluded.push(ExcludedFile {
                path: path.clone(),
                reason: "heuristic_only_lexical_match".to_string(),
                provenance: vec![CodeGraphProvenance {
                    source: "lexical_fallback".to_string(),
                    detail: "matched intent/question token by file path only".to_string(),
                    path: Some(path.clone()),
                }],
            });
        }
        if excluded.len() >= 20 {
            break;
        }
    }
    excluded
}

fn lexical_tokens(query: &CodeGraphQuery) -> BTreeSet<String> {
    let text = [query.intent.as_deref(), query.question.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let stop = [
        "change", "changed", "question", "optional", "short", "task", "intent", "code", "what",
        "where", "which", "should", "read", "file", "files",
    ];
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .map(str::to_ascii_lowercase)
        .filter(|token| token.len() >= 4 && !stop.contains(&token.as_str()))
        .collect()
}

fn attach_governance_rows(
    snapshot: &mut GraphSnapshot,
    governance: &GovernanceMetadata,
    repo: &CodeGraphRepoIdentity,
    commit: &str,
) {
    for file in &mut snapshot.files {
        file.repo_id = repo.id.clone();
        file.commit_sha = commit.to_string();
        file.owner = governance.owner_for_path(&file.path);
        file.test_lane = governance
            .test_for_path(&file.path)
            .map(|rule| rule.lane.clone());
        file.proof_lanes = file.test_lane.iter().cloned().collect();
        file.generated_zone = governance
            .generated_zone_for_path(&file.path)
            .map(|zone| zone.path);
        file.editable = governance
            .generated_zone_for_path(&file.path)
            .is_none_or(|zone| zone.manual_edits);
        file.provenance_json = serde_json::to_string(&vec![CodeGraphProvenance {
            source: "rust_cargo_index".to_string(),
            detail: "file discovered by Rust/Cargo workspace index".to_string(),
            path: Some(file.path.clone()),
        }])
        .unwrap_or_else(|_| "[]".to_string());
    }

    for loaded in &governance.loaded_files {
        snapshot.files.push(FileRow {
            repo_id: repo.id.clone(),
            commit_sha: commit.to_string(),
            path: loaded.path.clone(),
            crate_name: None,
            language: "governance".to_string(),
            owner: governance.owner_for_path(&loaded.path),
            test_lane: governance
                .test_for_path(&loaded.path)
                .map(|rule| rule.lane.clone()),
            proof_lanes: match governance.test_for_path(&loaded.path) {
                Some(rule) => vec![rule.lane.clone()],
                None => Vec::new(),
            },
            generated_zone: governance
                .generated_zone_for_path(&loaded.path)
                .map(|zone| zone.path),
            editable: governance
                .generated_zone_for_path(&loaded.path)
                .is_none_or(|zone| zone.manual_edits),
            provenance_json: serde_json::to_string(&vec![CodeGraphProvenance {
                source: "governance_ingestion".to_string(),
                detail: format!("loaded {}", loaded.kind),
                path: Some(loaded.path.clone()),
            }])
            .unwrap_or_else(|_| "[]".to_string()),
        });
        snapshot.governance.push(GovernanceRow {
            repo_id: repo.id.clone(),
            commit_sha: commit.to_string(),
            path: loaded.path.clone(),
            kind: loaded.kind.clone(),
            digest: loaded.digest.clone(),
            loaded: true,
        });
    }
    snapshot.files.sort_by(|a, b| a.path.cmp(&b.path));
    snapshot.files.dedup_by(|a, b| a.path == b.path);
}

fn manifest_relative_path(relative_root: &str) -> String {
    if relative_root == "." || relative_root.is_empty() {
        "Cargo.toml".to_string()
    } else {
        format!("{relative_root}/Cargo.toml")
    }
}

pub(super) fn normalize_changed_paths(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .map(|path| path.trim().trim_start_matches("./").replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(super) fn epoch_millis() -> u128 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(_) => 0,
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// Query accepted by the compatibility REST/MCP oracle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegraphQuery {
    /// Repo-relative changed paths to analyze for impact.
    #[serde(default)]
    pub changed_paths: Vec<String>,
    /// Optional symbol to resolve and collect references for.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Optional crate to inspect for reverse dependencies.
    #[serde(default)]
    pub crate_name: Option<String>,
    /// Limit for symbol search results.
    #[serde(default = "default_limit")]
    pub limit: usize,
}

/// Oracle response consumed by older codegraph clients and newer agent repair flows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegraphImpactPack {
    pub schema_version: String,
    pub provenance: CodegraphProvenance,
    pub impact: CodegraphImpact,
    pub symbols: Vec<SymbolRow>,
    pub definition: Option<SymbolRow>,
    pub references: Vec<SymbolRefRow>,
    pub reverse_deps: Vec<String>,
    pub required_reads: Vec<String>,
    pub proof_lanes: Vec<String>,
    pub suggested_commands: Vec<String>,
    pub misses: Vec<CodegraphMiss>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegraphProvenance {
    pub storage_schema: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegraphImpact {
    pub changed_crates: BTreeSet<String>,
    pub affected_crates: BTreeSet<String>,
    pub affected_symbols: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodegraphMiss {
    pub code: String,
    pub purpose: String,
    pub reason: String,
    pub common_fixes: Vec<String>,
    pub docs_url: String,
    pub repair_hint: String,
}

/// Build an oracle pack from a persisted codegraph store.
pub fn query_store(store: &CodeGraphStore, query: &CodegraphQuery) -> Result<CodegraphImpactPack> {
    let snapshot = store.load_snapshot()?;
    let schema_version = store.schema_version()?;
    Ok(query_snapshot(snapshot, schema_version, query))
}

/// Build an oracle pack from an already-loaded snapshot. This is the shared
/// deterministic path used by tests and the MCP memory backend.
#[must_use]
pub fn query_snapshot(
    snapshot: GraphSnapshot,
    schema_version: String,
    query: &CodegraphQuery,
) -> CodegraphImpactPack {
    let graph = CodeGraph::from_snapshot(snapshot);
    let symbols = match query.symbol.as_deref() {
        Some(symbol) => graph.search_symbols(symbol, query.limit),
        None => Vec::new(),
    };
    let definition = query
        .symbol
        .as_deref()
        .and_then(|symbol| graph.definition(symbol));
    let references = match query.symbol.as_deref() {
        Some(symbol) => graph.references(symbol),
        None => Vec::new(),
    };
    let reverse_deps = match query.crate_name.as_deref() {
        Some(name) => graph.reverse_deps(name),
        None => Vec::new(),
    };

    let mut changed_crates = BTreeSet::new();
    for path in &query.changed_paths {
        if let Some(crate_name) = crate_from_path(path, graph.snapshot()) {
            changed_crates.insert(crate_name);
        }
    }
    let mut affected_crates = changed_crates.clone();
    for crate_name in &changed_crates {
        for dependent in graph.reverse_deps(crate_name) {
            affected_crates.insert(dependent);
        }
    }
    let affected_symbols = graph
        .snapshot()
        .symbols
        .iter()
        .filter(|row| affected_crates.contains(&row.crate_name))
        .map(|row| row.symbol.clone())
        .collect();

    let mut required_reads = Vec::new();
    required_reads.extend(query.changed_paths.iter().cloned());
    if let Some(definition) = &definition {
        required_reads.push(definition.file.clone());
    }
    required_reads.extend(references.iter().map(|row| row.ref_file.clone()));
    required_reads.sort();
    required_reads.dedup();

    let mut misses = Vec::new();
    if query.symbol.is_some() && definition.is_none() {
        misses.push(miss(
            "codegraph_symbol_miss",
            "resolve codegraph symbol",
            "the requested symbol was not present in the current codegraph snapshot",
            "rerun `jeryu-codegraph index` and retry the query",
        ));
    }
    if query.crate_name.is_some() && reverse_deps.is_empty() {
        misses.push(miss(
            "codegraph_reverse_deps_empty",
            "resolve reverse dependency impact",
            "the requested crate has no recorded direct reverse dependencies",
            "rerun `jeryu-codegraph index` before treating this as final",
        ));
    }

    CodegraphImpactPack {
        schema_version: "codegraph.query/v1".to_string(),
        provenance: CodegraphProvenance {
            storage_schema: schema_version,
            source: "jeryu-codegraph/current-storage".to_string(),
        },
        impact: CodegraphImpact {
            changed_crates,
            affected_crates,
            affected_symbols,
        },
        symbols,
        definition,
        references,
        reverse_deps,
        required_reads,
        proof_lanes: vec![
            "rtk cargo test -p jeryu-codegraph -p jeryu-mcp --jobs 40 code".to_string(),
            "rtk bash ops/ci/codegraph-oracle.sh".to_string(),
        ],
        suggested_commands: vec![
            "rtk cargo run -p jeryu-codegraph -- index".to_string(),
            "rtk bash ops/ci/codegraph-oracle.sh".to_string(),
        ],
        misses,
    }
}

fn crate_from_path(path: &str, snapshot: &GraphSnapshot) -> Option<String> {
    snapshot
        .symbols
        .iter()
        .filter(|row| path.starts_with(row.file.trim_end_matches("src/lib.rs")))
        .max_by_key(|row| row.file.len())
        .map(|row| row.crate_name.clone())
        .or_else(|| {
            snapshot
                .symbols
                .iter()
                .find(|row| row.file == path)
                .map(|row| row.crate_name.clone())
        })
}

fn miss(code: &str, purpose: &str, reason: &str, repair_hint: &str) -> CodegraphMiss {
    CodegraphMiss {
        code: code.to_string(),
        purpose: purpose.to_string(),
        reason: reason.to_string(),
        common_fixes: vec![
            "refresh the codegraph SQLite snapshot".to_string(),
            "rerun the codegraph oracle proof lane".to_string(),
        ],
        docs_url: "docs/errors.md#not-found".to_string(),
        repair_hint: repair_hint.to_string(),
    }
}

fn default_limit() -> usize {
    20
}

impl From<CodeGraphError> for CodegraphMiss {
    fn from(error: CodeGraphError) -> Self {
        miss(
            "codegraph_storage_error",
            "load codegraph query evidence",
            &error.to_string(),
            "rerun `jeryu-codegraph index`, then rerun the codegraph oracle proof lane",
        )
    }
}
