//! Deterministic integration tests for jeryu-codegraph.

use std::path::PathBuf;

use jeryu_codegraph::{
    CodeGraph, CodeGraphStore, CrateDepRow, GraphSnapshot, QueryOptions, RepoIdentity, Slice,
    SymbolRow, enforce_export_slice_from_diff,
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
fn persist_repo_commit_is_scoped() {
    let path = unique_db("scoped");
    let store = CodeGraphStore::open(&path).unwrap();
    let repo = RepoIdentity {
        repo_id: "repo-a".into(),
        owner: "owner".into(),
        name: "repo-a".into(),
    };
    let first = GraphSnapshot {
        symbols: vec![SymbolRow {
            crate_name: "first".into(),
            file: "crates/first/src/lib.rs".into(),
            symbol: "First".into(),
            kind: "public".into(),
            is_public: true,
            line: 1,
        }],
        crate_deps: vec![],
    };
    let second = GraphSnapshot {
        symbols: vec![SymbolRow {
            crate_name: "second".into(),
            file: "crates/second/src/lib.rs".into(),
            symbol: "Second".into(),
            kind: "public".into(),
            is_public: true,
            line: 1,
        }],
        crate_deps: vec![],
    };

    store
        .persist_repo_commit(
            &repo,
            "refs/heads/main",
            "commit-a",
            ".",
            &QueryOptions::default(),
            &first,
        )
        .unwrap();
    store
        .persist_repo_commit(
            &repo,
            "refs/heads/main",
            "commit-b",
            ".",
            &QueryOptions::default(),
            &second,
        )
        .unwrap();

    assert_eq!(
        store
            .load_snapshot_for_scope(&repo, "commit-a", &QueryOptions::default())
            .unwrap(),
        first
    );
    assert_eq!(
        store
            .load_snapshot_for_scope(&repo, "commit-b", &QueryOptions::default())
            .unwrap(),
        second
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn query_or_refresh_hits_cache_and_writes_outbox_once() {
    let path = unique_db("cache");
    let store = CodeGraphStore::open(&path).unwrap();
    let repo = RepoIdentity::default();
    let scope = QueryOptions::default();
    let snapshot = GraphSnapshot {
        symbols: vec![SymbolRow {
            crate_name: "cache".into(),
            file: "crates/cache/src/lib.rs".into(),
            symbol: "Cache".into(),
            kind: "public".into(),
            is_public: true,
            line: 1,
        }],
        crate_deps: vec![],
    };
    let mut calls = 0usize;

    let first = store
        .query_or_refresh(
            &repo,
            "refs/heads/main",
            "commit-cache",
            ".",
            &scope,
            || {
                calls += 1;
                Ok(snapshot.clone())
            },
        )
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(
        first.1.cache_status,
        jeryu_codegraph::CacheStatus::Refreshed
    );
    assert!(first.1.outbox_event_id.is_some());

    let second = store
        .query_or_refresh(
            &repo,
            "refs/heads/main",
            "commit-cache",
            ".",
            &scope,
            || {
                calls += 1;
                Ok(GraphSnapshot::default())
            },
        )
        .unwrap();
    assert_eq!(calls, 1, "cache hit should not rebuild the snapshot");
    assert_eq!(second.1.cache_status, jeryu_codegraph::CacheStatus::Hit);
    assert!(second.1.outbox_event_id.is_none());
    assert_eq!(
        store
            .load_governance_records(&repo, "commit-cache")
            .unwrap()
            .len(),
        0
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn legacy_migration_backup_uses_pre_schema_v1_marker() {
    let path = unique_db("legacy");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE codegraph_symbols (
                crate TEXT NOT NULL,
                file TEXT NOT NULL,
                symbol TEXT NOT NULL,
                kind TEXT NOT NULL,
                is_public INTEGER NOT NULL,
                line INTEGER NOT NULL
            );
            CREATE TABLE codegraph_crate_deps (
                crate TEXT NOT NULL,
                depends_on TEXT NOT NULL
            );
            INSERT INTO codegraph_symbols
                (crate, file, symbol, kind, is_public, line)
                VALUES ('legacy', 'src/lib.rs', 'Legacy', 'public', 1, 7);
            INSERT INTO codegraph_crate_deps
                (crate, depends_on)
                VALUES ('legacy', 'dep');
            "#,
        )
        .unwrap();
    }

    let store = CodeGraphStore::open(&path).unwrap();
    let loaded = store.load_snapshot().unwrap();
    assert_eq!(loaded.symbols[0].symbol, "Legacy");
    assert_eq!(loaded.crate_deps[0].depends_on, "dep");

    let receipt_path = path.with_extension("migration.json");
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&receipt_path).unwrap()).unwrap();
    let backup_path = receipt["backup_path"].as_str().unwrap();
    assert!(
        backup_path.contains(".pre-schema-v1-"),
        "unexpected backup path: {backup_path}"
    );
    assert!(
        backup_path.ends_with(".bak"),
        "unexpected backup path: {backup_path}"
    );
    assert!(std::path::Path::new(backup_path).exists());

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&receipt_path);
    let _ = std::fs::remove_file(backup_path);
}

#[test]
fn malformed_json_fails_typed_storage_error() {
    let path = unique_db("malformed");
    let store = CodeGraphStore::open(&path).unwrap();
    let repo = RepoIdentity::default();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA ignore_check_constraints = ON;")
        .unwrap();
    conn.execute(
        "INSERT INTO codegraph_repos (repo_id, owner, name) VALUES (?1, ?2, ?3)",
        rusqlite::params![&repo.repo_id, &repo.owner, &repo.name],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO codegraph_files \
         (repo_id, commit_sha, path, crate, language, owner, test_lane, proof_lanes_json, generated_zone, editable, provenance_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            "local",
            "commit-json",
            "crates/a/src/lib.rs",
            "a",
            "rust",
            "owner",
            "fast",
            "{not-json}",
            0,
            1,
            "{still-not-json}",
        ],
    )
    .unwrap();

    let err = store.load_file_records(&repo, "commit-json").unwrap_err();
    assert!(
        err.to_string().contains("storage error"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn query_options_validate_bounds() {
    assert!(QueryOptions::default().validate().is_ok());
    assert!(
        QueryOptions {
            max_tokens: 0,
            proof_lanes: vec![],
        }
        .validate()
        .is_err()
    );
    assert!(
        QueryOptions {
            max_tokens: 60_001,
            proof_lanes: vec![],
        }
        .validate()
        .is_err()
    );
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
