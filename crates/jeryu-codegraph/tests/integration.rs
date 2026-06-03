//! Deterministic integration tests for jeryu-codegraph.

use std::path::PathBuf;

use jeryu_codegraph::{
    CodeGraph, CodeGraphStore, CrateDepRow, GraphSnapshot, Slice, SymbolRow,
    enforce_export_slice_from_diff,
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
    };
    store.persist(&snapshot).unwrap();
    let loaded = store.load_snapshot().unwrap();
    assert_eq!(loaded, snapshot);

    // Persist is idempotent (delete-all then re-insert).
    store.persist(&snapshot).unwrap();
    let loaded_again = store.load_snapshot().unwrap();
    assert_eq!(loaded_again, snapshot);

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
