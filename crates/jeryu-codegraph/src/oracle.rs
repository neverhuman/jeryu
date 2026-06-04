//! Compatibility oracle facade over the current codegraph storage.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::graph::CodeGraph;
use crate::storage::{CodeGraphStore, GraphSnapshot, SymbolRefRow, SymbolRow};
use crate::{Result, error::CodeGraphError};

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
    let symbols = query
        .symbol
        .as_deref()
        .map(|symbol| graph.search_symbols(symbol, query.limit))
        .unwrap_or_default();
    let definition = query
        .symbol
        .as_deref()
        .and_then(|symbol| graph.definition(symbol));
    let references = query
        .symbol
        .as_deref()
        .map(|symbol| graph.references(symbol))
        .unwrap_or_default();
    let reverse_deps = query
        .crate_name
        .as_deref()
        .map(|name| graph.reverse_deps(name))
        .unwrap_or_default();

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
