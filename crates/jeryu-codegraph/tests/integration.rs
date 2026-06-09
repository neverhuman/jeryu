//! Deterministic integration tests for jeryu-codegraph.

use std::path::PathBuf;

use jeryu_codegraph::{
    CodeGraph, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore,
    CodegraphQuery, CrateDepRow, GraphSnapshot, Slice, SymbolRefRow, SymbolRow,
    ToolBuildScanConfig, enforce_export_slice_from_diff, query_store, scan_tool_build_clusters,
};

fn unique_db(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("codegraph-test-{tag}-{nanos}.sqlite"));
    dir
}

fn unique_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("codegraph-test-{tag}-{nanos}"));
    dir
}

fn write_file(root: &std::path::Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("file parent")).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn fixture_workspace() -> PathBuf {
    let root = unique_dir("oracle-fixture");
    write_file(
        &root,
        "Cargo.toml",
        r#"[workspace]
members = ["crates/core", "crates/api"]
"#,
    );
    write_file(
        &root,
        "crates/core/Cargo.toml",
        r#"[package]
name = "core_lib"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &root,
        "crates/core/src/lib.rs",
        "pub fn core_api() {}\nmod security_notes;\n",
    );
    write_file(
        &root,
        "crates/core/src/security_notes.rs",
        "fn internal_security_notes() {}\n",
    );
    write_file(
        &root,
        "crates/core/src/generated/schema.rs",
        "pub fn generated_api() {}\n",
    );
    write_file(
        &root,
        "crates/api/Cargo.toml",
        r#"[package]
name = "api_lib"
version = "0.1.0"
edition = "2024"

[dependencies]
core_lib = { path = "../core" }
"#,
    );
    write_file(&root, "crates/api/src/lib.rs", "pub fn api() {}\n");
    write_file(&root, "AGENTS.md", "fixture guidance\n");
    write_file(
        &root,
        "agent/owner-map.json",
        r#"{
  "workspace": "fixture",
  "owners": {
    "crates/core/": "core-owner",
    "crates/api/": "api-owner",
    "agent/": "agent-owner",
    "AGENTS.md": "docs-owner"
  }
}
"#,
    );
    write_file(
        &root,
        "agent/test-map.json",
        r#"{
  "workspace": "fixture",
  "tests": {
    "crates/core/": {
      "command": "cargo test -p core_lib",
      "purpose": "verify core crate",
      "lane": "api"
    },
    "crates/api/": {
      "command": "cargo test -p api_lib",
      "purpose": "verify api crate",
      "lane": "api"
    }
  }
}
"#,
    );
    write_file(
        &root,
        "agent/generated-zones.toml",
        r#"[[zones]]
path = "crates/core/src/generated/**"
generator = "fixture-gen"
manual_edits = false
"#,
    );
    write_file(
        &root,
        "agent/proof-lanes.toml",
        r#"[lanes.api]
required = ["cargo test -p core_lib"]
blocks_merge = true
"#,
    );
    root
}

#[test]
fn persist_round_trip() {
    let path = unique_db("roundtrip");
    let store = CodeGraphStore::open(&path).unwrap();
    let snapshot = GraphSnapshot {
        symbols: vec![
            SymbolRow {
                crate_name: "jeryu-codegraph".into(),
                file: "crates/jeryu-codegraph/src/lib.rs".into(),
                symbol: "CodeGraph".into(),
                kind: "public".into(),
                is_public: true,
                line: 0,
            },
            SymbolRow {
                crate_name: "jeryu-codegraph".into(),
                file: "crates/jeryu-codegraph/src/slice.rs".into(),
                symbol: "Slice".into(),
                kind: "public".into(),
                is_public: true,
                line: 0,
            },
        ],
        crate_deps: vec![CrateDepRow {
            crate_name: "jeryu-codegraph".into(),
            depends_on: "jeryu-rustjet".into(),
        }],
        symbol_refs: vec![SymbolRefRow {
            crate_name: "jeryu-codegraph".into(),
            file: "crates/jeryu-codegraph/src/lib.rs".into(),
            symbol: "CodeGraph".into(),
            ref_file: "crates/jeryu-codegraph/tests/integration.rs".into(),
            ref_line: 42,
            ref_kind: "type".into(),
        }],
        ..Default::default()
    };
    store.persist(&snapshot).unwrap();
    let loaded = store.load_snapshot().unwrap();
    assert_eq!(loaded, snapshot);
    assert_eq!(store.schema_version().unwrap(), "3");
    assert_eq!(store.references("CodeGraph").unwrap(), snapshot.symbol_refs);
    assert_eq!(
        store.reverse_deps("jeryu-rustjet").unwrap(),
        vec!["jeryu-codegraph".to_string()]
    );

    // Persist is idempotent (delete-all then re-insert).
    store.persist(&snapshot).unwrap();
    let loaded_again = store.load_snapshot().unwrap();
    assert_eq!(loaded_again, snapshot);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn tool_build_scan_persists_clusters_and_ignore_feedback() {
    let root = unique_dir("tool-build");
    let repeated = r#"
pub fn alpha_retry(input: &str) -> Result<String, String> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let response = call_remote(input);
        if response.is_ok() {
            return response;
        }
        if attempts > 3 {
            return Err("failed".to_string());
        }
    }
}
"#;
    write_file(&root, "crates/a/src/lib.rs", repeated);
    write_file(
        &root,
        "crates/b/src/lib.rs",
        &repeated.replace("alpha_retry", "beta_retry"),
    );
    let config = ToolBuildScanConfig {
        window_lines: 5,
        min_normalized_tokens: 12,
        min_occurrences: 2,
        max_file_bytes: 64 * 1024,
        max_clusters: 10,
    };
    let report = scan_tool_build_clusters(&root, "local/jeryu", "commit-a", config).unwrap();
    assert_eq!(report.scanned_files, 2);
    assert!(
        report.clusters.iter().any(|cluster| {
            cluster.occurrence_count >= 2 && cluster.insight.contains("rust normalized window")
        }),
        "expected at least one repeated normalized Rust window"
    );

    let path = unique_db("tool-build");
    let store = CodeGraphStore::open(&path).unwrap();
    store.persist_tool_build_report(&report).unwrap();
    let clusters = store
        .tool_build_clusters(Some("local/jeryu"), 10, false)
        .unwrap();
    assert!(!clusters.is_empty());
    let ignored = store
        .ignore_tool_build_cluster(&clusters[0].cluster_id, "fixture boilerplate", "test")
        .unwrap();
    assert_eq!(ignored.reason, "fixture boilerplate");
    assert!(
        store
            .tool_build_clusters(Some("local/jeryu"), 10, false)
            .unwrap()
            .iter()
            .all(|cluster| cluster.cluster_id != ignored.cluster_id)
    );
    assert!(
        store
            .tool_build_clusters(Some("local/jeryu"), 10, true)
            .unwrap()
            .iter()
            .any(|cluster| cluster
                .ignored
                .as_ref()
                .is_some_and(|i| i.reason == "fixture boilerplate"))
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn oracle_query_pack_includes_provenance_refs_and_lanes() {
    let path = unique_db("oracle");
    let store = CodeGraphStore::open(&path).unwrap();
    let snapshot = GraphSnapshot {
        symbols: vec![
            SymbolRow {
                crate_name: "jeryu-codegraph".into(),
                file: "crates/jeryu-codegraph/src/lib.rs".into(),
                symbol: "CodeGraph".into(),
                kind: "public".into(),
                is_public: true,
                line: 10,
            },
            SymbolRow {
                crate_name: "jeryu-mcp".into(),
                file: "crates/jeryu-mcp/src/backend/memory.rs".into(),
                symbol: "MemoryBackend".into(),
                kind: "public".into(),
                is_public: true,
                line: 20,
            },
        ],
        crate_deps: vec![CrateDepRow {
            crate_name: "jeryu-mcp".into(),
            depends_on: "jeryu-codegraph".into(),
        }],
        symbol_refs: vec![SymbolRefRow {
            crate_name: "jeryu-codegraph".into(),
            file: "crates/jeryu-codegraph/src/lib.rs".into(),
            symbol: "CodeGraph".into(),
            ref_file: "crates/jeryu-mcp/src/backend/memory.rs".into(),
            ref_line: 7,
            ref_kind: "type".into(),
        }],
        ..Default::default()
    };
    store.persist(&snapshot).unwrap();

    let pack = query_store(
        &store,
        &CodegraphQuery {
            changed_paths: vec!["crates/jeryu-codegraph/src/lib.rs".into()],
            symbol: Some("CodeGraph".into()),
            crate_name: Some("jeryu-codegraph".into()),
            limit: 10,
        },
    )
    .unwrap();

    assert_eq!(pack.provenance.storage_schema, "3");
    assert_eq!(pack.definition.as_ref().unwrap().symbol, "CodeGraph");
    assert_eq!(
        pack.references[0].ref_file,
        "crates/jeryu-mcp/src/backend/memory.rs"
    );
    assert_eq!(pack.reverse_deps, vec!["jeryu-mcp"]);
    assert!(
        pack.required_reads
            .contains(&"crates/jeryu-codegraph/src/lib.rs".to_string())
    );
    assert!(
        pack.proof_lanes
            .iter()
            .any(|lane| lane.contains("codegraph-oracle"))
    );
    assert!(pack.misses.is_empty());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn index_real_workspace_root_and_impact() {
    // The worktree root is two levels up from this crate dir.
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.parent().unwrap().parent().unwrap();

    let workspace = jeryu_rustjet::WorkspaceGraph::load(root).unwrap();
    let graph = CodeGraph::index_workspace(&workspace).unwrap();

    // Our own crate's public symbols should be indexed.
    let snapshot = graph.snapshot();
    assert!(
        snapshot
            .symbols
            .iter()
            .any(|s| s.crate_name == "jeryu-codegraph"),
        "expected jeryu-codegraph symbols in the index"
    );

    // jeryu-codegraph depends on jeryu-rustjet (workspace-internal edge).
    assert!(
        graph
            .crate_dependencies()
            .get("jeryu-codegraph")
            .is_some_and(|deps| deps.contains("jeryu-rustjet")),
        "expected jeryu-codegraph -> jeryu-rustjet dep edge"
    );

    // Changing a rustjet file affects rustjet itself and its reverse-deps,
    // which include jeryu-codegraph.
    let report = graph.impact_of(
        &workspace,
        &["crates/jeryu-rustjet/src/graph.rs".to_string()],
    );
    assert!(report.changed_crates.contains("jeryu-rustjet"));
    assert!(report.affected_crates.contains("jeryu-rustjet"));
    assert!(
        report.affected_crates.contains("jeryu-codegraph"),
        "jeryu-codegraph is a reverse dependency of jeryu-rustjet"
    );
}

#[test]
fn slice_deny_out_of_slice() {
    let slice = Slice::new(["crates/jeryu-codegraph"]);
    let changed = vec!["crates/jeryu-core/x.rs".to_string()];
    let err = slice.slice_permits(&changed).expect_err("must deny");
    assert_eq!(err.out_of_slice_paths, vec!["crates/jeryu-core/x.rs"]);
    assert_eq!(
        slice.first_out_of_slice(&changed),
        Some("crates/jeryu-core/x.rs".to_string())
    );
}

#[test]
fn slice_allow_in_prefix() {
    let slice = Slice::new(["crates/jeryu-codegraph"]);
    let changed = vec![
        "crates/jeryu-codegraph/src/lib.rs".to_string(),
        "crates/jeryu-codegraph/Cargo.toml".to_string(),
    ];
    assert!(slice.slice_permits(&changed).is_ok());
    assert_eq!(slice.first_out_of_slice(&changed), None);
}

#[test]
fn slice_empty_allowed_denies() {
    let slice = Slice::default();
    let changed = vec!["crates/jeryu-codegraph/src/lib.rs".to_string()];
    assert!(slice.slice_permits(&changed).is_err());
}

#[test]
fn tautology_regression_core_not_permitted_by_api() {
    // PROOF the corrected predicate rejects what the tautology bug accepted:
    // changed=crates/jeryu-core/x.rs is NOT permitted by allowed=crates/jeryu-api.
    let slice = Slice::new(["crates/jeryu-api"]);
    let changed = vec!["crates/jeryu-core/x.rs".to_string()];
    let err = slice
        .slice_permits(&changed)
        .expect_err("corrected predicate must deny crates/jeryu-core/x.rs under crates/jeryu-api");
    assert_eq!(err.out_of_slice_paths, vec!["crates/jeryu-core/x.rs"]);
}

#[test]
fn export_gate_deny_and_allow() {
    // Deny path.
    let deny = enforce_export_slice_from_diff(
        &["crates/jeryu-core/x.rs".to_string()],
        &["crates/jeryu-codegraph".to_string()],
    );
    let denied = deny.expect_err("must deny");
    assert_eq!(denied.out_of_slice_paths, vec!["crates/jeryu-core/x.rs"]);

    // Allow path.
    let allow = enforce_export_slice_from_diff(
        &["crates/jeryu-codegraph/src/slice.rs".to_string()],
        &["crates/jeryu-codegraph".to_string()],
    );
    assert_eq!(
        allow.unwrap(),
        vec!["crates/jeryu-codegraph/src/slice.rs".to_string()]
    );
}

#[test]
fn oracle_query_builds_auditable_impact_pack() {
    let root = fixture_workspace();
    let db = unique_db("oracle-rich");
    let store = CodeGraphStore::open(&db).unwrap();
    let service = CodeGraphService::new(&root, store.clone());
    let pack = service
        .query(
            CodeGraphRepoIdentity {
                id: "fixture/repo".into(),
                owner: "fixture".into(),
                name: "repo".into(),
            },
            "abc123",
            CodeGraphQuery {
                ref_name: "main".into(),
                changed_paths: vec!["crates/core/src/generated/schema.rs".into()],
                intent: Some("edit generated core api".into()),
                question: Some("security impact?".into()),
                max_tokens: Some(12_000),
            },
        )
        .unwrap();

    assert_eq!(pack.repo.id, "fixture/repo");
    assert_eq!(pack.ref_name, "main");
    assert_eq!(pack.commit, "abc123");
    assert!(pack.changed_crates.contains(&"core_lib".to_string()));
    assert!(pack.affected_crates.contains(&"api_lib".to_string()));
    assert!(
        pack.affected_symbols
            .iter()
            .any(|symbol| symbol.crate_name == "api_lib" && symbol.symbol == "api")
    );

    let generated = pack
        .must_read_files
        .iter()
        .find(|file| file.path == "crates/core/src/generated/schema.rs")
        .expect("changed generated file is must-read");
    assert_eq!(generated.owner.as_deref(), Some("core-owner"));
    assert_eq!(generated.proof_lanes, vec!["api"]);
    assert!(!generated.editable);
    assert_eq!(
        generated
            .generated_zone
            .as_ref()
            .map(|zone| zone.path.as_str()),
        Some("crates/core/src/generated/**")
    );

    assert!(
        pack.should_read_files
            .iter()
            .any(|file| file.path == "agent/proof-lanes.toml")
    );
    assert!(
        pack.proof_lanes
            .iter()
            .any(|lane| lane.lane == "api" && lane.blocks_merge)
    );
    assert!(
        pack.suggested_commands
            .contains(&"cargo test -p core_lib".to_string())
    );

    assert!(
        pack.excluded_files
            .iter()
            .any(|file| file.path == "crates/core/src/security_notes.rs")
    );
    assert!(
        !pack
            .must_read_files
            .iter()
            .any(|file| file.path == "crates/core/src/security_notes.rs"),
        "heuristic-only security path must not be authoritative must-read context"
    );

    for file in pack
        .must_read_files
        .iter()
        .chain(pack.should_read_files.iter())
    {
        assert!(
            !file.provenance.is_empty(),
            "{} should carry provenance",
            file.path
        );
    }

    let json = serde_json::to_value(&pack).unwrap();
    for key in [
        "repo",
        "ref",
        "commit",
        "changed_paths",
        "changed_crates",
        "affected_crates",
        "affected_symbols",
        "must_read_files",
        "should_read_files",
        "proof_lanes",
        "suggested_commands",
        "excluded_files",
        "graph_stats",
        "residual_risk",
        "provenance",
        "index_receipt",
    ] {
        assert!(json.get(key).is_some(), "stable pack key {key}");
    }

    let loaded = store.load_snapshot().unwrap();
    assert!(!loaded.files.is_empty());
    assert_eq!(loaded.governance.len(), 5);
    assert_eq!(loaded.index_runs.len(), 1);

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_dir_all(&root);
}
