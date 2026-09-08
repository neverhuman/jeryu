//! End-to-end tests for the all-Rust finder pipeline: scan -> dossier ->
//! propose, including the propose idempotency contract and registry
//! comment preservation.

use std::path::Path;
use std::process::Command;

fn finder() -> Command {
    Command::new(env!("CARGO_BIN_EXE_jeryu-tool-finder"))
}

fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, contents).unwrap();
}

const SHARED_HELPER: &str = r#"
pub fn alpha_handler(req: Request, ctx: &Context) -> Response {
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

#[test]
fn scan_dossier_propose_round_trip() {
    let split = tempfile::tempdir().expect("split root");
    // Two fixture repos carrying the shared helper under different names.
    write_file(split.path(), "repo-a/src/lib.rs", SHARED_HELPER);
    write_file(
        split.path(),
        "repo-b/src/lib.rs",
        &SHARED_HELPER.replace("alpha_handler", "beta_handler"),
    );
    write_file(
        split.path(),
        "repos.manifest.toml",
        &format!(
            "[[repo]]\npath = \"{}\"\nname = \"repo-a\"\n\n[[repo]]\npath = \"{}\"\nname = \"repo-b\"\n",
            split.path().join("repo-a").display(),
            split.path().join("repo-b").display(),
        ),
    );
    // A minimal sibling jeryu-tool registry for propose to append to.
    write_file(
        split.path(),
        "jeryu-tool/tools-registry.toml",
        "# Registry header comment that MUST survive appends.\nschema_version = \"1\"\n",
    );

    let work = split.path().join("finder-work");
    std::fs::create_dir_all(&work).unwrap();
    let db = split.path().join("codegraph.sqlite");
    let out = work.join("dossiers").join("clusters.json");

    // Scan the fixture family.
    let scan = finder()
        .current_dir(&work)
        .args([
            "scan",
            "--manifest",
            split.path().join("repos.manifest.toml").to_str().unwrap(),
            "--db",
            db.to_str().unwrap(),
            "--repo-id",
            "family/fixture",
            "--window-lines",
            "5",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("run scan");
    assert!(
        scan.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&scan.stderr)
    );
    assert!(out.is_file(), "clusters.json written");

    // Enrich into dossiers.
    let dossier = finder()
        .current_dir(&work)
        .args([
            "dossier",
            "--input",
            out.to_str().unwrap(),
            "--out-dir",
            work.join("dossiers").to_str().unwrap(),
        ])
        .output()
        .expect("run dossier");
    assert!(
        dossier.status.success(),
        "dossier failed: {}",
        String::from_utf8_lossy(&dossier.stderr)
    );
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(work.join("dossiers/index.json")).unwrap())
            .unwrap();
    let cluster_id = index[0]["cluster_id"]
        .as_str()
        .expect("cluster id")
        .to_string();
    assert!(index[0]["anticipated_loc_saved"].as_u64().unwrap() > 0);
    assert!(
        work.join("dossiers")
            .join(format!("{cluster_id}.md"))
            .is_file(),
        "per-cluster markdown written"
    );

    // Propose files a registry entry + build task.
    let propose = finder()
        .current_dir(&work)
        .args([
            "propose",
            &cluster_id,
            "--jeryu-tool",
            split.path().join("jeryu-tool").to_str().unwrap(),
            "--dossiers",
            work.join("dossiers/index.json").to_str().unwrap(),
        ])
        .output()
        .expect("run propose");
    assert!(
        propose.status.success(),
        "propose failed: {}",
        String::from_utf8_lossy(&propose.stderr)
    );
    let registry =
        std::fs::read_to_string(split.path().join("jeryu-tool/tools-registry.toml")).unwrap();
    assert!(
        registry.starts_with("# Registry header comment"),
        "textual append preserves comments"
    );
    assert!(registry.contains(&format!("origin_cluster = \"{cluster_id}\"")));
    assert!(registry.contains("status = \"proposed\""));
    assert!(registry.contains("candidate_repos = [\n  \"repo-a\",\n  \"repo-b\",\n]"));
    let tasks: Vec<_> = std::fs::read_dir(split.path().join("jeryu-tool/tasks"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(tasks.len(), 1, "exactly one build task filed");

    // Idempotency: re-proposing the same cluster is a no-op.
    let replay = finder()
        .current_dir(&work)
        .args([
            "propose",
            &cluster_id,
            "--jeryu-tool",
            split.path().join("jeryu-tool").to_str().unwrap(),
            "--dossiers",
            work.join("dossiers/index.json").to_str().unwrap(),
        ])
        .output()
        .expect("replay propose");
    assert!(replay.status.success());
    assert!(
        String::from_utf8_lossy(&replay.stdout).contains("already proposed"),
        "replay must be a no-op"
    );
    let registry_after =
        std::fs::read_to_string(split.path().join("jeryu-tool/tools-registry.toml")).unwrap();
    assert_eq!(registry, registry_after, "no duplicate registry entry");

    // Dry-run never writes.
    let dry = finder()
        .current_dir(&work)
        .args([
            "propose",
            "missing-cluster",
            "--jeryu-tool",
            split.path().join("jeryu-tool").to_str().unwrap(),
            "--dossiers",
            work.join("dossiers/index.json").to_str().unwrap(),
            "--dry-run",
        ])
        .output()
        .expect("dry-run propose");
    assert!(!dry.status.success(), "unknown cluster must fail");
}

#[test]
fn dossier_selftest_passes_against_bundled_fixture() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = finder()
        .current_dir(repo_root)
        .args(["dossier", "--selftest"])
        .output()
        .expect("run selftest");
    assert!(
        output.status.success(),
        "selftest failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("dossier selftest ok"));
}
