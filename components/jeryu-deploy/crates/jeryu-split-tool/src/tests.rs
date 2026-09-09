use super::*;

fn git(path: &Path, arguments: &[&str]) -> String {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn coverage_patterns_match_legacy_assignment_shapes() {
    let patterns = vec![
        "crates/core/**".to_owned(),
        "README.md".to_owned(),
        "docs/*.md".to_owned(),
        "tests/fixture?.json".to_owned(),
    ];
    assert!(is_covered("crates/core/src/lib.rs", &patterns));
    assert!(is_covered("README.md", &patterns));
    assert!(is_covered("docs/release.md", &patterns));
    assert!(is_covered("tests/fixture1.json", &patterns));
    assert!(!is_covered("crates/other/src/lib.rs", &patterns));
    assert!(!is_covered("README.md.bak", &patterns));
}

#[test]
fn coverage_patterns_match_python_fnmatch_classes_and_unicode_characters() {
    assert!(wildcard_matches("docs/[a-c].md", "docs/b.md"));
    assert!(!wildcard_matches("docs/[a-c].md", "docs/z.md"));
    assert!(wildcard_matches("docs/[!0-9].md", "docs/x.md"));
    assert!(!wildcard_matches("docs/[!0-9].md", "docs/7.md"));
    assert!(wildcard_matches("docs/?.md", "docs/é.md"));
    assert!(wildcard_matches("docs/[.md", "docs/[.md"));
}

#[test]
fn lock_rejects_missing_and_noncanonical_commits() {
    let valid: Value = toml::from_str(
        r#"
                [[repo]]
                name = "jeryu"
                github_slug = "neverhuman/jeryu"
                local_path = "/tmp/jeryu"
                tag = "jeryu-v5.0.0-split.0"
                commit = "0123456789abcdef0123456789abcdef01234567"
                required_check = "jeryu/required"
            "#,
    )
    .unwrap();
    verify_lock_value(&valid).unwrap();

    let invalid: Value = toml::from_str(
        r#"
                [[repo]]
                name = "jeryu"
                github_slug = "neverhuman/jeryu"
                local_path = "/tmp/jeryu"
                tag = "jeryu-v5.0.0-split.0"
                commit = "ABC"
                required_check = ""
            "#,
    )
    .unwrap();
    let error = verify_lock_value(&invalid).unwrap_err().to_string();
    assert!(error.contains("jeryu missing required_check"));
    assert!(error.contains("jeryu commit is not a sha: ABC"));
}

#[test]
fn manifest_rejects_duplicate_repositories() {
    let duplicate: Value = toml::from_str(
        r#"
                required_repos = ["jeryu", "missing"]
                [[repo]]
                name = "jeryu"
                path = "/tmp/jeryu"
                github_slug = "neverhuman/jeryu"
                jeryu_slug = "jeryu/jeryu"
                profile = "portal"
                default_branch = "main"
                current_tag = "jeryu-v5.0.0-split.0"
                required_check = "jeryu/required"
                has_jeryu_std = true
                [[repo]]
                name = "jeryu"
                path = "/tmp/jeryu-duplicate"
                github_slug = "neverhuman/jeryu"
                jeryu_slug = "jeryu/jeryu"
                profile = "portal"
                default_branch = "main"
                current_tag = "jeryu-v5.0.0-split.0"
                required_check = "jeryu/required"
                has_jeryu_std = true
            "#,
    )
    .unwrap();
    assert!(
        validate_manifest_value(&duplicate, false)
            .unwrap_err()
            .to_string()
            .contains("duplicate repo name")
    );
}

#[test]
fn manifest_path_validation_is_physical_and_required_inventory_is_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let repo = temporary.path().join("jeryu");
    fs::create_dir_all(repo.join("agent")).unwrap();
    for path in ["AGENTS.md", "agent/owner-map.json", "agent/test-map.json"] {
        fs::write(repo.join(path), b"{}\n").unwrap();
    }
    let source = format!(
        r#"
                required_repos = ["jeryu"]
                [[repo]]
                name = "jeryu"
                path = "{}"
                github_slug = "neverhuman/jeryu"
                jeryu_slug = "jeryu/jeryu"
                profile = "portal"
                default_branch = "main"
                current_tag = "jeryu-v5.0.0-split.0"
                required_check = "jeryu/required"
                has_jeryu_std = true
            "#,
        repo.display()
    );
    let manifest: Value = toml::from_str(&source).unwrap();
    validate_manifest_value(&manifest, true).unwrap();

    fs::remove_file(repo.join("agent/test-map.json")).unwrap();
    assert!(
        validate_manifest_value(&manifest, true)
            .unwrap_err()
            .to_string()
            .contains("jeryu missing agent/test-map.json")
    );

    let missing_required: Value = toml::from_str(&source.replace(
        "required_repos = [\"jeryu\"]",
        "required_repos = [\"jeryu\", \"jeryu-core\"]",
    ))
    .unwrap();
    assert!(
        validate_manifest_value(&missing_required, false)
            .unwrap_err()
            .to_string()
            .contains("manifest missing required repos: jeryu-core")
    );
}

#[test]
fn source_tree_reader_uses_the_declared_bare_fallback() {
    let temporary = tempfile::tempdir().unwrap();
    let working = temporary.path().join("working");
    let bare = temporary.path().join("source.git");
    fs::create_dir(&working).unwrap();
    git(&working, &["init", "--quiet"]);
    fs::write(working.join("README.md"), b"source\n").unwrap();
    git(&working, &["add", "README.md"]);
    git(
        &working,
        &[
            "-c",
            "user.name=Jeryu Test",
            "-c",
            "user.email=test@jeryu.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let head = git(&working, &["rev-parse", "HEAD"]);
    let output = ProcessCommand::new("git")
        .args(["clone", "--bare", "--no-local"])
        .arg(&working)
        .arg(&bare)
        .output()
        .unwrap();
    assert!(output.status.success());

    let (files, reader) =
        git_tree(&temporary.path().join("missing-source"), Some(&bare), &head).unwrap();
    assert_eq!(files, ["README.md"]);
    assert_eq!(reader, bare.display().to_string());
}

#[test]
fn source_coverage_fails_when_a_tracked_path_is_unassigned() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("source");
    fs::create_dir(&source).unwrap();
    git(&source, &["init", "--quiet"]);
    fs::write(source.join("unassigned.txt"), b"unassigned\n").unwrap();
    git(&source, &["add", "unassigned.txt"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Jeryu Test",
            "-c",
            "user.email=test@jeryu.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let head = git(&source, &["rev-parse", "HEAD"]);
    let manifest = temporary.path().join("manifest.toml");
    fs::write(
            &manifest,
            format!(
                "source_root = {:?}\nsource_sha = {:?}\nshared_source_paths = [\"README.md\"]\n\n[[repo]]\nname = \"jeryu\"\npath = {:?}\ngithub_slug = \"neverhuman/jeryu\"\njeryu_slug = \"jeryu/jeryu\"\nprofile = \"portal\"\ndefault_branch = \"main\"\ncurrent_tag = \"jeryu-v5.0.0-split.0\"\nrequired_check = \"jeryu/required\"\nhas_jeryu_std = true\n",
                source.display().to_string(),
                head,
                source.display().to_string(),
            ),
        )
        .unwrap();

    let error = source_coverage(&manifest, true).unwrap_err().to_string();
    assert_eq!(error, "source coverage failed");
}

#[test]
fn source_coverage_json_preserves_sorted_pass_report_bytes() {
    let report = SourceCoverageReport {
        missing: Vec::new(),
        missing_count: 0,
        patterns: 3,
        schema_version: "jeryu.split.source-coverage/v1",
        source_git_dir: None,
        source_reader: "/source".to_owned(),
        source_root: "/source".to_owned(),
        source_sha: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        status: "pass",
        tracked_files: 7,
    };

    assert_eq!(
        source_coverage_json(&report).unwrap(),
        r#"{
  "missing": [],
  "missing_count": 0,
  "patterns": 3,
  "schema_version": "jeryu.split.source-coverage/v1",
  "source_git_dir": null,
  "source_reader": "/source",
  "source_root": "/source",
  "source_sha": "0123456789abcdef0123456789abcdef01234567",
  "status": "pass",
  "tracked_files": 7
}"#
    );
}

#[test]
fn source_coverage_json_preserves_sorted_fail_report_bytes() {
    let report = SourceCoverageReport {
        missing: vec!["src/unassigned.rs".to_owned()],
        missing_count: 1,
        patterns: 2,
        schema_version: "jeryu.split.source-coverage/v1",
        source_git_dir: Some("/source.git".to_owned()),
        source_reader: "/source.git".to_owned(),
        source_root: "/missing-source".to_owned(),
        source_sha: "89abcdef0123456789abcdef0123456789abcdef".to_owned(),
        status: "fail",
        tracked_files: 5,
    };

    assert_eq!(
        source_coverage_json(&report).unwrap(),
        r#"{
  "missing": [
    "src/unassigned.rs"
  ],
  "missing_count": 1,
  "patterns": 2,
  "schema_version": "jeryu.split.source-coverage/v1",
  "source_git_dir": "/source.git",
  "source_reader": "/source.git",
  "source_root": "/missing-source",
  "source_sha": "89abcdef0123456789abcdef0123456789abcdef",
  "status": "fail",
  "tracked_files": 5
}"#
    );
}

#[test]
fn fleet_plan_preserves_manifest_order_and_selects_one_lane() {
    let manifest: Value = toml::from_str(
        r#"
                required_repos = ["first", "second"]
                [[repo]]
                name = "first"
                path = "/tmp/first"
                github_slug = "neverhuman/first"
                jeryu_slug = "jeryu/first"
                profile = "portal"
                default_branch = "main"
                current_tag = "first-v5.0.0-split.0"
                required_check = "first/required"
                has_jeryu_std = true
                [[repo]]
                name = "second"
                path = "/tmp/second"
                github_slug = "neverhuman/second"
                jeryu_slug = "jeryu/second"
                profile = "core"
                default_branch = "main"
                current_tag = "second-v5.0.0-split.0"
                required_check = "second/required"
                has_jeryu_std = true
            "#,
    )
    .unwrap();

    let score = fleet_entries(&manifest, false).unwrap();
    assert_eq!(
        score[0],
        (
            "first".to_owned(),
            PathBuf::from("/tmp/first"),
            "score".to_owned()
        )
    );
    assert_eq!(
        score[1],
        (
            "second".to_owned(),
            PathBuf::from("/tmp/second"),
            "score".to_owned()
        )
    );
    assert!(
        fleet_entries(&manifest, true)
            .unwrap()
            .iter()
            .all(|(_, _, lane)| lane == "check")
    );
}

#[test]
fn governed_process_failure_is_not_swallowed() {
    let error = run_process(ProcessCommand::new("sh").args(["-c", "exit 17"]), "fixture")
        .unwrap_err()
        .to_string();
    assert!(error.contains("governed command for fixture failed"));
    assert!(error.contains("17"));
}
