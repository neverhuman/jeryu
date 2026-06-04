//! Behavioral tests for every governance gate: a passing fixture and a failing
//! fixture per subcommand, asserting the exact signal lines and pass/fail
//! outcome the Python predecessors produced.

use std::fs;
use std::path::{Path, PathBuf};

use jeryu_mapcheck::{agent_maps, db_boundary, docs, fixtures, generated_zones, owner_test_map};

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn pass() -> PathBuf {
    fixtures_root().join("pass")
}

fn fail() -> PathBuf {
    fixtures_root().join("fail")
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, contents).expect("write file");
}

// --- owner-test-map (check-owner-test-map.py) -----------------------------

#[test]
fn owner_test_map_passes_on_complete_maps() {
    let p = pass().join("agent");
    let report =
        owner_test_map(&p.join("owner-map.json"), &p.join("test-map.json")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["owner/test map ok".to_string()]);
}

#[test]
fn owner_test_map_fails_on_missing_roots_and_bad_entry() {
    let f = fail().join("agent");
    let report =
        owner_test_map(&f.join("owner-map.json"), &f.join("test-map.json")).expect("gate runs");
    assert!(!report.ok, "expected fail");
    // owner map only has agent/** and bins/**, so the other 10 glob roots are missing.
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.starts_with("missing owner paths:") && l.contains("'crates/**'")),
        "missing owner paths line absent: {:?}",
        report.lines
    );
    // test map only has agent/** -> 11 missing test roots.
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.starts_with("missing test paths:") && l.contains("'tests/**'")),
        "missing test paths line absent: {:?}",
        report.lines
    );
    // bins/** owner entry has empty owners -> bad owner entry.
    assert!(
        report
            .lines
            .iter()
            .any(|l| l.starts_with("bad owner entries:")),
        "bad owner entries line absent: {:?}",
        report.lines
    );
}

// --- agent-maps (check-agent-maps.py) -------------------------------------

#[test]
fn agent_maps_passes_on_complete_maps() {
    let p = pass().join("agent");
    let report =
        agent_maps(&p.join("owner-map.json"), &p.join("test-map.json")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(
        report.lines,
        vec!["agent maps cover repository paths".to_string()]
    );
}

#[test]
fn agent_maps_fails_on_missing_roots() {
    let f = fail().join("agent");
    let report =
        agent_maps(&f.join("owner-map.json"), &f.join("test-map.json")).expect("gate runs");
    assert!(!report.ok, "expected fail");
    assert_eq!(report.lines.len(), 1, "single SystemExit-style line");
    let line = &report.lines[0];
    assert!(
        line.starts_with("missing owner=") && line.contains("tests="),
        "unexpected line: {line}"
    );
    // owner roots present: agent, bins -> config..tests missing on owner side.
    assert!(
        line.contains("'config'"),
        "owner-missing roots absent: {line}"
    );
    // test roots present: agent only -> bins..tests missing on test side.
    assert!(
        line.contains("'crates'"),
        "test-missing roots absent: {line}"
    );
}

// --- generated-zones (check-generated-zones.py) ---------------------------

#[test]
fn generated_zones_passes_on_correct_manifest() {
    let report = generated_zones(&pass().join("agent/generated-zones.toml")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["generated zones ok".to_string()]);
}

#[test]
fn generated_zones_fails_on_manual_edits_and_wrong_generator() {
    let report = generated_zones(&fail().join("agent/generated-zones.toml")).expect("gate runs");
    assert!(!report.ok, "expected fail");
    assert!(
        report.lines.contains(
            &"generated zone docs/generated/** must set manual_edits = false".to_string()
        ),
        "manual_edits diagnostic absent: {:?}",
        report.lines
    );
    assert!(
        report.lines.iter().any(|l| l.contains(
            "generated zone receipts/generated/** has generator 'wrong-generator', expected 'jeryu-cache-service'"
        )),
        "wrong-generator diagnostic absent: {:?}",
        report.lines
    );
}

// --- fixtures (check-fixtures.py) -----------------------------------------

#[test]
fn fixtures_passes_and_skips_apple_double() {
    // pass/fix contains good.json (valid) and ._sidecar.json (invalid, must skip).
    let report = fixtures(&pass().join("fix")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["fixtures ok".to_string()]);
}

#[test]
fn fixtures_errors_on_malformed_json() {
    // The Python json.loads raises on broken.json; our gate returns an Err.
    let err = fixtures(&fail().join("fix")).expect_err("expected parse failure");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("broken.json"),
        "error should name the file: {msg}"
    );
}

// --- docs (check-docs.py) -------------------------------------------------

#[test]
fn docs_passes_when_all_markers_present() {
    let report = docs(&pass().join("docs")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["docs ok".to_string()]);
}

#[test]
fn docs_fails_on_missing_marker_and_missing_file() {
    let report = docs(&fail().join("docs")).expect("gate runs");
    assert!(!report.ok, "expected fail");
    assert!(
        report
            .lines
            .contains(&"README.md missing marker: Phase 12".to_string()),
        "missing-marker diagnostic absent: {:?}",
        report.lines
    );
    assert!(
        report
            .lines
            .contains(&"missing required doc: docs/engineering_spec.md".to_string()),
        "missing-doc diagnostic absent: {:?}",
        report.lines
    );
}

// --- db-boundary ----------------------------------------------------------

#[test]
fn db_boundary_allows_configured_truth_owner() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write(
        root,
        "agent/boundaries.toml",
        r#"[db]
truth_owner = "jeryu-core"
"#,
    );
    write(
        root,
        "crates/jeryu-core/src/storage.rs",
        &format!("use {}::Connection;", ["rus", "qlite"].concat()),
    );

    let report = db_boundary(root, &root.join("agent/boundaries.toml")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["db boundary ok".to_string()]);
}

#[test]
fn db_boundary_allows_multiple_truth_owners() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write(
        root,
        "agent/boundaries.toml",
        r#"[db]
truth_owner = "jeryu-core"
truth_owners = ["jeryu-core", "jeryu-codegraph"]
"#,
    );
    write(
        root,
        "crates/jeryu-codegraph/src/storage.rs",
        &format!("use {}::Connection;", ["rus", "qlite"].concat()),
    );

    let report = db_boundary(root, &root.join("agent/boundaries.toml")).expect("gate runs");
    assert!(report.ok, "expected pass, got {report:?}");
    assert_eq!(report.lines, vec!["db boundary ok".to_string()]);
}

#[test]
fn db_boundary_flags_driver_use_outside_truth_owner() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    write(
        root,
        "agent/boundaries.toml",
        r#"[db]
truth_owner = "jeryu-core"
"#,
    );
    write(
        root,
        "crates/jeryu-api/src/web.rs",
        &format!("use {}::Connection;", ["rus", "qlite"].concat()),
    );

    let report = db_boundary(root, &root.join("agent/boundaries.toml")).expect("gate runs");
    assert!(!report.ok, "expected fail");
    assert_eq!(report.lines[0], "sqlite-driver boundary violations:");
    assert!(
        report
            .lines
            .contains(&"  crates/jeryu-api/src/web.rs".to_string()),
        "violation path absent: {:?}",
        report.lines
    );
}
