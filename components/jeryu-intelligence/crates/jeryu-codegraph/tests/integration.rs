//! Deterministic integration tests for jeryu-codegraph.

use std::path::PathBuf;

use jeryu_codegraph::{
    CodeGraph, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore,
    CodegraphQuery, CrateDepRow, GraphSnapshot, Slice, SymbolRefRow, SymbolRow,
    ToolBuildScanConfig, enforce_export_slice_from_diff, query_store, scan_tool_build_clusters,
    scan_tool_build_family,
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
    assert_eq!(store.schema_version().unwrap(), "4");
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
        min_repo_count: 1,
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
fn tool_build_family_scan_merges_clusters_across_repos() {
    // The same normalized window lives in two SEPARATE repo roots. The family
    // scan must fold both occurrences into ONE cluster spanning both repos.
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
    let repo_a = unique_dir("tool-build-family-a");
    let repo_b = unique_dir("tool-build-family-b");
    write_file(&repo_a, "crates/a/src/lib.rs", repeated);
    // Rename the identifier so only the NORMALIZED window matches — this proves
    // the merge keys on the fingerprint, not on raw text.
    write_file(
        &repo_b,
        "crates/b/src/lib.rs",
        &repeated.replace("alpha_retry", "beta_retry"),
    );
    let config = ToolBuildScanConfig {
        window_lines: 5,
        min_normalized_tokens: 12,
        min_occurrences: 2,
        max_file_bytes: 64 * 1024,
        max_clusters: 10,
        min_repo_count: 2,
    };
    let roots = vec![
        ("local/repo-a".to_string(), repo_a.clone()),
        ("local/repo-b".to_string(), repo_b.clone()),
    ];
    let report =
        scan_tool_build_family(&roots, "family/jeryu-split", "working-tree", config).unwrap();
    assert_eq!(report.repo_id, "family/jeryu-split");
    assert_eq!(report.scanned_files, 2);

    let cross_repo = report
        .clusters
        .iter()
        .find(|cluster| cluster.repo_count >= 2)
        .expect("expected a cross-repo cluster spanning both roots");
    assert_eq!(cross_repo.repo_count, 2);
    let repos: std::collections::BTreeSet<&str> = cross_repo
        .occurrences
        .iter()
        .map(|occ| occ.repo_id.as_str())
        .collect();
    assert!(repos.contains("local/repo-a"));
    assert!(repos.contains("local/repo-b"));

    // Every surviving cluster must satisfy the min_repo_count=2 filter: a window
    // that lived in only one repo must NOT be returned by the family scan.
    assert!(
        report
            .clusters
            .iter()
            .all(|cluster| cluster.repo_count >= 2),
        "family scan must drop single-repo clusters when min_repo_count=2"
    );

    let _ = std::fs::remove_dir_all(&repo_a);
    let _ = std::fs::remove_dir_all(&repo_b);
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

    assert_eq!(pack.provenance.storage_schema, "4");
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

/// Dense fixture whose 8-line windows clear the default 36-token floor.
/// MUST stay byte-identical to the corpus used to pin the golden ids below.
const PARITY_HANDLER: &str = r#"pub fn alpha_handler(req: Request, ctx: &Context) -> Response {
    let parsed = validate_input(req.body(), ctx.schema(), MAX_BYTES).expect("validated");
    let token = ctx.auth().issue_token(parsed.user_id(), Scope::ReadWrite, EXPIRY_SECS);
    let record = Record::new(parsed.id(), parsed.payload(), token.claims(), now_ms());
    audit_log(ctx.logger(), "create", record.id(), record.actor(), record.checksum());
    let stored = ctx.store().insert(record.clone(), WriteMode::Durable).map_err(wrap_err)?;
    notify_subscribers(ctx.bus(), Topic::Created, stored.id(), stored.version());
    metrics_incr(ctx.metrics(), "records_created_total", 1, &[("kind", "create")]);
    Response::created(stored.id(), stored.version(), etag_for(stored.checksum()))
}
"#;

/// Golden parity pin: the v1 binary (pre-rewrite `jeryu-codegraph tool-build
/// scan --window-lines 8`) produced EXACTLY these cluster ids, fingerprints,
/// and scores on this fixture. The rewritten scanner must keep them
/// byte-identical or every persisted cluster id and ignore row breaks.
#[test]
fn tool_build_v1_fingerprints_are_byte_stable() {
    let root = unique_dir("tool-build-parity");
    write_file(&root, "crates/a/src/lib.rs", PARITY_HANDLER);
    write_file(
        &root,
        "crates/b/src/lib.rs",
        &PARITY_HANDLER.replace("alpha_handler", "beta_handler"),
    );
    let config = ToolBuildScanConfig {
        window_lines: 8,
        min_normalized_tokens: 36,
        min_occurrences: 2,
        max_file_bytes: 512 * 1024,
        max_clusters: 20,
        min_repo_count: 1,
    };
    let report = scan_tool_build_clusters(&root, "parity", "c1", config).unwrap();
    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.clusters.len(), 2);

    let first = &report.clusters[0];
    assert_eq!(first.cluster_id, "toolbuild-efdd149d2df5fdf0");
    assert_eq!(
        first.fingerprint,
        "efdd149d2df5fdf0e18e65c1002e7191ca3bddf217ad13a6712c814f3229d357"
    );
    assert_eq!(first.score, 820);
    assert_eq!(first.occurrence_count, 2);
    assert_eq!(first.total_lines, 16);
    assert_eq!(first.file_count, 2);
    assert_eq!(first.occurrences[0].start_line, 2);
    assert_eq!(first.occurrences[0].end_line, 9);
    assert_eq!(first.occurrences[0].normalized_token_count, 151);

    let second = &report.clusters[1];
    assert_eq!(second.cluster_id, "toolbuild-e94583fddcc261b6");
    assert_eq!(
        second.fingerprint,
        "e94583fddcc261b63c41a8877d17c4e0fd04619ab2b14dcb7e60cd34664563f0"
    );
    assert_eq!(second.score, 748);

    let _ = std::fs::remove_dir_all(&root);
}

/// End-to-end v2 system scan: nested + pathless manifests are discovered,
/// gitignored junk never enters the index, the duplicated-block ladder merges
/// into ONE honest maximal cluster, scaffold clusters are categorized (not
/// dropped), generated zones are skipped, `system/host` rows coexist with
/// `family/...` rows, ignores propagate onto merged clusters, progress phases
/// arrive in order, and the output is thread-count invariant.
#[test]
fn tool_build_system_scan_end_to_end() {
    use std::sync::Mutex;

    use jeryu_codegraph::{
        ToolBuildCategory, ToolBuildScanOptions, ToolBuildScanPhase, discover_system_repo_roots,
        scan_tool_build_system,
    };

    let parent = unique_dir("system-scan-parent");

    // Family "alpha-split": manifest at the family root with explicit paths.
    let repo_a = parent.join("alpha-split/repo-a");
    write_file(
        &parent.join("alpha-split"),
        "repos.manifest.toml",
        &format!(
            "[[repo]]\npath = \"{}\"\nname = \"repo-a\"\ngithub_slug = \"nh/repo-a\"\n",
            repo_a.display()
        ),
    );
    // Family "beta-split": nested manifest with NO path keys (jankurai shape);
    // repos resolve as siblings of the manifest's parent directory. The
    // "repo-a-live" entry is a SECOND checkout of the same logical repo
    // (github remote collides with alpha's github_slug, case-insensitively)
    // and must be deduped — otherwise two checkouts of one repo would
    // manufacture fake cross-repo duplication.
    let live_checkout = parent.join("beta-split/repo-a-live");
    write_file(
        &parent.join("beta-split/beta"),
        "repos.manifest.toml",
        &format!(
            "[[repo]]\nname = \"beta\"\n\n[[repo]]\nname = \"repo-b\"\n\n\
             [[repo]]\nname = \"repo-a-live\"\npath = \"{}\"\n\
             github = \"https://github.com/NH/repo-a.git\"\n",
            live_checkout.display()
        ),
    );
    let repo_b = parent.join("beta-split/repo-b");
    std::fs::create_dir_all(parent.join("beta-split/beta/src")).unwrap();
    write_file(&live_checkout, "src/handlers.rs", PARITY_HANDLER);

    // The duplicated handler plus a shared tail line: 11 normalized lines at
    // window 8 give window starts 0..=3; start 0 carries the renamed
    // identifier (call:alpha_handler vs call:gamma_handler) so only starts
    // 1..=3 repeat across repos — a ladder of exactly 3 chainable windows.
    // Comments/blanks interleave to prove chaining runs in normalized space.
    let block_a = format!(
        "// preamble comment\n\n{PARITY_HANDLER}\npub fn shared_tail(x: Foo) -> Bar {{ convert(x.alpha(), x.beta(), x.gamma(), DEFAULT_TIMEOUT) }}\n"
    );
    write_file(&repo_a, "src/handlers.rs", &block_a);
    write_file(
        &repo_b,
        "src/service.rs",
        &block_a.replace("alpha_handler", "gamma_handler"),
    );

    // Managed scaffold: identical CI lane script in both repos.
    let scaffold = r#"set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
run_lane "fmt" cargo fmt --check --all
run_lane "clippy" cargo clippy --all-targets -- -D warnings
run_lane "test" cargo test --workspace --all-features
record_artifact "$ROOT/.artifacts/check.json" "$LANE_STATUS"
publish_check "$ROOT" "check" "$LANE_STATUS" "$CHECK_URL"
enforce_floor "$ROOT" "$LANE_STATUS" "$FLOOR" "$HARD_FINDINGS"
summarize "$ROOT/.artifacts" "check lane complete" "$LANE_STATUS"
exit_with "$LANE_STATUS"
"#;
    write_file(&repo_a, "ops/ci/check.sh", scaffold);
    write_file(&repo_b, "ops/ci/check.sh", scaffold);

    // Generated zone: identical generated content must NOT cluster.
    write_file(
        &repo_a,
        "agent/generated-zones.toml",
        "[[zones]]\npath = \"genzone/**\"\n",
    );
    write_file(
        &repo_b,
        "agent/generated-zones.toml",
        "[[zones]]\npath = \"genzone/**\"\n",
    );
    write_file(&repo_a, "genzone/gen.rs", PARITY_HANDLER);
    write_file(&repo_b, "genzone/gen.rs", PARITY_HANDLER);

    // Gitignored junk: identical content under an ignored dir in a REAL git
    // repo must never enter the index (git ls-files discovery).
    write_file(&repo_a, ".gitignore", "junk/\n");
    write_file(&repo_a, "junk/copy.rs", PARITY_HANDLER);
    let git_init = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo_a)
        .arg("init")
        .arg("-q")
        .status()
        .expect("git init");
    assert!(git_init.success());

    let roots = discover_system_repo_roots(std::slice::from_ref(&parent)).unwrap();
    let ids: Vec<&str> = roots.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["beta", "repo-a", "repo-b"]);

    let mut options = ToolBuildScanOptions::system_default();
    options.base.max_clusters = 50;
    options.threads = 1;

    // Pre-seed a family row to prove the system scan cannot stomp it.
    let db = unique_db("system-scan");
    let store = CodeGraphStore::open(&db).unwrap();
    let family_report = scan_tool_build_family(
        &[
            ("repo-a".to_string(), repo_a.clone()),
            ("repo-b".to_string(), repo_b.clone()),
        ],
        "family/fixture",
        "working-tree",
        ToolBuildScanConfig {
            min_repo_count: 2,
            ..ToolBuildScanConfig::default()
        },
    )
    .unwrap();
    assert!(!family_report.clusters.is_empty());
    store.persist_tool_build_report(&family_report).unwrap();
    // Ignore one PRE-MERGE window id so the merged cluster can inherit it.
    let window_id = &family_report.clusters[0].cluster_id;
    store
        .ignore_tool_build_cluster(window_id, "fixture noise", "test")
        .unwrap();

    let events = Mutex::new(Vec::new());
    let report = scan_tool_build_system(&roots, "system/host", "working-tree", &options, &|p| {
        events.lock().unwrap().push(p);
    })
    .unwrap();

    // Phases arrive in pipeline order.
    let phases: Vec<ToolBuildScanPhase> = {
        let events = events.lock().unwrap();
        events.iter().map(|event| event.phase).collect()
    };
    assert_eq!(phases.first(), Some(&ToolBuildScanPhase::Discover));
    assert!(phases.contains(&ToolBuildScanPhase::Scan));
    assert!(phases.contains(&ToolBuildScanPhase::Merge));
    assert_eq!(phases.last(), Some(&ToolBuildScanPhase::Finalize));
    let scan_pos = phases
        .iter()
        .position(|p| *p == ToolBuildScanPhase::Scan)
        .unwrap();
    let merge_pos = phases
        .iter()
        .position(|p| *p == ToolBuildScanPhase::Merge)
        .unwrap();
    assert!(scan_pos < merge_pos);

    // The handler ladder merged into ONE maximal cluster with honest spans.
    let handler = report
        .clusters
        .iter()
        .find(|cluster| cluster.language == "rust" && !cluster.member_cluster_ids.is_empty())
        .expect("merged rust handler cluster");
    assert_eq!(handler.occurrence_count, 2);
    assert_eq!(handler.repo_count, 2);
    assert_eq!(handler.category, ToolBuildCategory::ToolCandidate);
    assert!(
        handler.member_cluster_ids.len() >= 3,
        "ladder of 3+ windows chained"
    );
    let occ = &handler.occurrences[0];
    // Raw span covers the whole duplicated block (comment/blank interleave
    // proves chaining ran in normalized space).
    assert!(occ.end_line - occ.start_line + 1 >= 10);
    // No occurrence from gitignored junk or generated zones.
    assert!(
        report.clusters.iter().all(|cluster| cluster
            .occurrences
            .iter()
            .all(|o| !o.path.starts_with("junk/") && !o.path.starts_with("genzone/"))),
        "ignored/generated paths must never cluster"
    );

    // Scaffold repetition is visible but categorized, never a tool candidate.
    let scaffold_cluster = report
        .clusters
        .iter()
        .find(|cluster| {
            cluster
                .occurrences
                .iter()
                .any(|o| o.path == "ops/ci/check.sh")
        })
        .expect("scaffold cluster present");
    assert_eq!(
        scaffold_cluster.category,
        ToolBuildCategory::ManagedScaffold
    );

    // Families exist and cover the handler cluster.
    assert!(!report.families.is_empty());
    assert!(
        report
            .families
            .iter()
            .any(|family| family.cluster_ids.contains(&handler.cluster_id))
    );

    // Persist under system/host; family rows intact; merged ignore inherited.
    store.persist_tool_build_report(&report).unwrap();
    store.propagate_ignores_to_merged(&report.clusters).unwrap();
    let family_rows = store
        .tool_build_clusters(Some("family/fixture"), 100, true)
        .unwrap();
    assert_eq!(family_rows.len(), family_report.clusters.len());
    let system_rows = store
        .tool_build_clusters(Some("system/host"), 100, true)
        .unwrap();
    assert_eq!(system_rows.len(), report.clusters.len());
    let merged_row = system_rows
        .iter()
        .find(|cluster| cluster.member_cluster_ids.contains(window_id))
        .expect("merged cluster containing the ignored window id");
    assert!(
        merged_row
            .ignored
            .as_ref()
            .is_some_and(|i| i.reason.contains("inherited")),
        "merged cluster inherits the member window's ignore"
    );

    // Families recompute identically from persisted rows.
    let from_store = store
        .tool_build_families(Some("system/host"), 100, true)
        .unwrap();
    assert_eq!(from_store.len(), report.families.len());

    // Thread-count invariance: an 8-worker scan yields identical clusters.
    options.threads = 8;
    let report_mt =
        scan_tool_build_system(&roots, "system/host", "working-tree", &options, &|_| {}).unwrap();
    assert_eq!(report.clusters, report_mt.clusters);
    assert_eq!(report.families, report_mt.families);

    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_dir_all(&parent);
}

/// Opening a v3-shaped store (single-column PK, no category columns) migrates
/// it in place: rows and ignore joins survive, and the table gains the
/// composite primary key that lets family and system scans coexist.
#[test]
fn tool_build_storage_migrates_v3_to_v4() {
    let db = unique_db("migrate-v3");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            r#"
CREATE TABLE codegraph_tool_build_clusters (
    cluster_id         TEXT PRIMARY KEY,
    repo_id            TEXT NOT NULL,
    commit_sha         TEXT NOT NULL,
    fingerprint        TEXT NOT NULL,
    score              INTEGER NOT NULL,
    occurrence_count   INTEGER NOT NULL,
    repo_count         INTEGER NOT NULL,
    file_count         INTEGER NOT NULL,
    total_lines        INTEGER NOT NULL,
    language           TEXT NOT NULL,
    insight            TEXT NOT NULL,
    normalized_preview TEXT NOT NULL,
    occurrences_json   TEXT NOT NULL,
    created_at         TEXT NOT NULL
);
CREATE TABLE codegraph_tool_build_ignores (
    cluster_id TEXT PRIMARY KEY,
    reason     TEXT NOT NULL,
    ignored_by TEXT NOT NULL,
    ignored_at TEXT NOT NULL
);
CREATE TABLE codegraph_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO codegraph_meta (key, value) VALUES ('schema_version', '3');
INSERT INTO codegraph_tool_build_clusters VALUES
  ('toolbuild-old1', 'family/jeryu-split', 'sha1', 'fp1', 100, 2, 2, 2, 16,
   'rust', 'insight', 'preview', '[]', '1000');
INSERT INTO codegraph_tool_build_ignores VALUES
  ('toolbuild-old1', 'pre-migration ignore', 'operator', '1001');
"#,
        )
        .unwrap();
    }

    let store = CodeGraphStore::open(&db).unwrap();
    assert_eq!(store.schema_version().unwrap(), "4");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let ddl: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='codegraph_tool_build_clusters'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(ddl.contains("PRIMARY KEY (repo_id, cluster_id)"));

    let rows = store
        .tool_build_clusters(Some("family/jeryu-split"), 10, true)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cluster_id, "toolbuild-old1");
    assert!(
        rows[0]
            .ignored
            .as_ref()
            .is_some_and(|i| i.reason == "pre-migration ignore")
    );
    assert!(rows[0].member_cluster_ids.is_empty());

    // Re-opening is a no-op (shape already migrated).
    let again = CodeGraphStore::open(&db).unwrap();
    assert_eq!(again.tool_build_clusters(None, 10, true).unwrap().len(), 1);

    let _ = std::fs::remove_file(&db);
}
