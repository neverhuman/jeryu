//! Code graph: a symbol index plus crate dependency edges built from the
//! workspace, with impact analysis.
//!
//! Reuse-first: the workspace topology, path -> package mapping, and transitive
//! reverse-dependency walk all come from `jeryu_rustjet::WorkspaceGraph`. Public
//! symbol extraction reuses `jeryu_rustjet::PublicApiDetector` (which internally
//! runs the regex-free `public_symbols` extractor) so the symbol logic is never
//! duplicated here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use jeryu_rustjet::{PublicApiDetector, WorkspaceGraph};

use crate::error::{CodeGraphError, Result};
use crate::storage::{
    CodeGraphStore, CrateDepRow, FileRow, GraphSnapshot, SymbolRefRow, SymbolRow,
};

/// An in-memory code graph for a workspace.
#[derive(Debug, Clone, Default)]
pub struct CodeGraph {
    snapshot: GraphSnapshot,
    /// crate -> set of crates it depends on (workspace-internal only).
    crate_deps: BTreeMap<String, BTreeSet<String>>,
}

/// Result of impact analysis over a set of changed paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImpactReport {
    /// Crates directly owning a changed path.
    pub changed_crates: BTreeSet<String>,
    /// Crates transitively affected (reverse-deps of the changed crates).
    pub affected_crates: BTreeSet<String>,
    /// Public symbols owned by the affected crates.
    pub affected_symbols: BTreeSet<String>,
}

impl CodeGraph {
    /// Builds a code graph by walking each workspace package's `src/*.rs` files
    /// and indexing their public symbols, recording workspace-internal crate
    /// dependency edges.
    pub fn index(root: impl AsRef<Path>) -> Result<Self> {
        let workspace =
            WorkspaceGraph::load(root).map_err(|e| CodeGraphError::Workspace(e.to_string()))?;
        Self::index_workspace(&workspace)
    }

    /// Builds a code graph from an already-loaded `WorkspaceGraph`.
    pub fn index_workspace(workspace: &WorkspaceGraph) -> Result<Self> {
        let detector = PublicApiDetector::new();
        let mut symbols = Vec::new();
        let mut crate_deps_rows = Vec::new();
        let mut files: BTreeMap<String, FileRow> = BTreeMap::new();
        let mut crate_deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for package in workspace.packages() {
            let manifest_path = if package.relative_root == "." {
                "Cargo.toml".to_string()
            } else {
                format!("{}/Cargo.toml", package.relative_root)
            };
            files.entry(manifest_path.clone()).or_insert(FileRow {
                repo_id: String::new(),
                commit_sha: String::new(),
                path: manifest_path,
                crate_name: Some(package.name.clone()),
                language: "cargo_toml".to_string(),
                owner: None,
                test_lane: None,
                proof_lanes: Vec::new(),
                generated_zone: None,
                editable: true,
                provenance_json: "[]".to_string(),
            });

            // Record workspace-internal dependency edges.
            let deps = workspace.direct_dependencies_of(&package.name);
            let entry = crate_deps.entry(package.name.clone()).or_default();
            for dep in deps {
                entry.insert(dep.clone());
                crate_deps_rows.push(CrateDepRow {
                    crate_name: package.name.clone(),
                    depends_on: dep,
                });
            }

            // Walk src/*.rs and index public symbols via the rustjet detector.
            let src_dir = package.root.join("src");
            for relative in collect_rust_sources(&src_dir, &package.relative_root)? {
                files.entry(relative.clone()).or_insert(FileRow {
                    repo_id: String::new(),
                    commit_sha: String::new(),
                    path: relative.clone(),
                    crate_name: Some(package.name.clone()),
                    language: "rust".to_string(),
                    owner: None,
                    test_lane: None,
                    proof_lanes: Vec::new(),
                    generated_zone: None,
                    editable: true,
                    provenance_json: "[]".to_string(),
                });
                // `relative` is repo-relative (e.g. crates/foo/src/lib.rs).
                let inside = package
                    .path_inside_package(&relative)
                    .unwrap_or(relative.as_str());
                let Some(change) = detector.detect(package, inside) else {
                    continue;
                };
                for symbol in change.symbols {
                    symbols.push(SymbolRow {
                        crate_name: package.name.clone(),
                        file: relative.clone(),
                        symbol,
                        kind: "public".to_string(),
                        is_public: true,
                        line: 0,
                    });
                }
            }
        }

        crate_deps_rows.sort_by(|a, b| {
            (a.crate_name.as_str(), a.depends_on.as_str())
                .cmp(&(b.crate_name.as_str(), b.depends_on.as_str()))
        });
        symbols.sort_by(|a, b| {
            (a.crate_name.as_str(), a.file.as_str(), a.symbol.as_str()).cmp(&(
                b.crate_name.as_str(),
                b.file.as_str(),
                b.symbol.as_str(),
            ))
        });

        Ok(Self {
            snapshot: GraphSnapshot {
                symbols,
                crate_deps: crate_deps_rows,
                symbol_refs: Vec::new(),
                files: files.into_values().collect(),
                governance: Vec::new(),
                index_runs: Vec::new(),
            },
            crate_deps,
        })
    }

    /// Persists this graph's snapshot to the given store (atomic, see
    /// `CodeGraphStore::persist`).
    pub fn persist(&self, store: &CodeGraphStore) -> Result<()> {
        store.persist(&self.snapshot)
    }

    /// Loads a graph back from a persisted snapshot.
    pub fn from_snapshot(snapshot: GraphSnapshot) -> Self {
        let mut crate_deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for dep in &snapshot.crate_deps {
            crate_deps
                .entry(dep.crate_name.clone())
                .or_default()
                .insert(dep.depends_on.clone());
        }
        Self {
            snapshot,
            crate_deps,
        }
    }

    /// The underlying persistable snapshot.
    #[must_use]
    pub fn snapshot(&self) -> &GraphSnapshot {
        &self.snapshot
    }

    /// Computes the impact of a set of changed repo-relative paths.
    ///
    /// Reuses `WorkspaceGraph::package_for_path` to resolve changed crates and
    /// `transitive_reverse_dependencies_of` to expand to affected crates.
    pub fn impact_of(&self, workspace: &WorkspaceGraph, changed_paths: &[String]) -> ImpactReport {
        let mut changed_crates = BTreeSet::new();
        for path in changed_paths {
            if let Some(package) = workspace.package_for_path(path) {
                changed_crates.insert(package.name.clone());
            }
        }

        let mut affected_crates = changed_crates.clone();
        for crate_name in &changed_crates {
            for dependent in workspace.transitive_reverse_dependencies_of(crate_name) {
                affected_crates.insert(dependent);
            }
        }

        let mut affected_symbols = BTreeSet::new();
        for row in &self.snapshot.symbols {
            if affected_crates.contains(&row.crate_name) {
                affected_symbols.insert(row.symbol.clone());
            }
        }

        ImpactReport {
            changed_crates,
            affected_crates,
            affected_symbols,
        }
    }

    /// Workspace-internal dependency edges (crate -> dependencies).
    #[must_use]
    pub fn crate_dependencies(&self) -> &BTreeMap<String, BTreeSet<String>> {
        &self.crate_deps
    }

    /// Search in-memory symbols by symbol, file, or crate substring.
    #[must_use]
    pub fn search_symbols(&self, query: &str, limit: usize) -> Vec<SymbolRow> {
        let needle = query.to_ascii_lowercase();
        self.snapshot
            .symbols
            .iter()
            .filter(|row| {
                row.symbol.to_ascii_lowercase().contains(&needle)
                    || row.file.to_ascii_lowercase().contains(&needle)
                    || row.crate_name.to_ascii_lowercase().contains(&needle)
            })
            .take(limit.max(1))
            .cloned()
            .collect()
    }

    /// Return the first exact symbol definition.
    #[must_use]
    pub fn definition(&self, symbol: &str) -> Option<SymbolRow> {
        self.snapshot
            .symbols
            .iter()
            .find(|row| row.symbol == symbol)
            .cloned()
    }

    /// Return reference rows for an exact symbol name.
    #[must_use]
    pub fn references(&self, symbol: &str) -> Vec<SymbolRefRow> {
        self.snapshot
            .symbol_refs
            .iter()
            .filter(|row| row.symbol == symbol)
            .cloned()
            .collect()
    }

    /// Return direct reverse dependencies for `crate_name`.
    #[must_use]
    pub fn reverse_deps(&self, crate_name: &str) -> Vec<String> {
        self.crate_deps
            .iter()
            .filter_map(|(candidate, deps)| deps.contains(crate_name).then_some(candidate.clone()))
            .collect()
    }
}

/// Collects repo-relative paths of `*.rs` files under `src_dir`, recursively.
///
/// `relative_root` is the package's repo-relative root (e.g. `crates/foo`,
/// or `.` for a root package). Returned paths are repo-relative and use `/`.
fn collect_rust_sources(src_dir: &Path, relative_root: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if !src_dir.exists() {
        return Ok(out);
    }
    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| CodeGraphError::Index {
            path: dir.display().to_string(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| CodeGraphError::Index {
                path: dir.display().to_string(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| CodeGraphError::Index {
                path: path.display().to_string(),
                source,
            })?;
            if file_type.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && let Some(rel) = repo_relative(relative_root, src_dir, &path)
            {
                out.push(rel);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Builds a repo-relative, forward-slash path for a source file inside a
/// package whose repo-relative root is `relative_root`.
fn repo_relative(relative_root: &str, src_dir: &Path, file: &Path) -> Option<String> {
    let inside_src = file.strip_prefix(src_dir).ok()?;
    let mut parts: Vec<String> = Vec::new();
    if relative_root != "." && !relative_root.is_empty() {
        parts.push(relative_root.to_string());
    }
    parts.push("src".to_string());
    for component in inside_src.components() {
        parts.push(component.as_os_str().to_string_lossy().into_owned());
    }
    Some(parts.join("/"))
}
