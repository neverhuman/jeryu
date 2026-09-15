use super::*;
use jeryu_core::CreateRepositoryRequest;
use jeryu_gitd::refs::GitRef;
use std::fs;

#[cfg(unix)]
fn write_test_jankurai(path: &Path) {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all(b"#!/usr/bin/env bash\nprintf 'jankurai 1.6.11\\n'\n")
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();

    // llvm-cov can race an immediately created executable on Linux and return
    // ETXTBSY even after the writer has closed. Stabilize only this disposable
    // fixture; production identity verification deliberately remains a single
    // fail-closed hash-and-execute attempt.
    for attempt in 0..25 {
        match Command::new(path).arg("--version").output() {
            Ok(output) => {
                assert!(output.status.success());
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    "jankurai 1.6.11"
                );
                return;
            }
            Err(error) if error.raw_os_error() == Some(26) && attempt < 24 => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("test jankurai fixture did not become executable: {error}"),
        }
    }
    unreachable!("bounded fixture readiness loop must return or panic");
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_out(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

#[cfg(unix)]
#[test]
fn governed_jankurai_identity_rejects_version_digest_and_physical_substitution() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let good = temp.path().join("jankurai-good");
    write_test_jankurai(&good);
    let good_sha = hex::encode(Sha256::digest(fs::read(&good).unwrap()));

    let good_result = verify_jankurai_identity(&good, "jankurai 1.6.11", &good_sha);
    assert!(good_result.is_ok(), "{good_result:?}");
    assert!(verify_jankurai_identity(&good, "jankurai 1.6.10", &good_sha).is_err());
    assert!(verify_jankurai_identity(&good, "jankurai 1.6.11", &"0".repeat(64)).is_err());

    let linked = temp.path().join("jankurai-linked");
    symlink(&good, &linked).unwrap();
    assert!(verify_jankurai_identity(&linked, "jankurai 1.6.11", &good_sha).is_err());

    let hard_target = temp.path().join("jankurai-hard-target");
    let hard_alias = temp.path().join("jankurai-hard-alias");
    fs::copy(&good, &hard_target).unwrap();
    fs::hard_link(&hard_target, &hard_alias).unwrap();
    assert!(verify_jankurai_identity(&hard_target, "jankurai 1.6.11", &good_sha).is_err());
}

fn governed_receipt(binary: &Path, binary_sha: &str) -> serde_json::Value {
    let governed: serde_json::Value =
        serde_json::from_str(GOVERNED_JANKURAI_INSTALLATION_RECEIPT_JSON).unwrap();
    serde_json::json!({
        "binary": {
            "sha256": binary_sha,
            "version_output": "jankurai 1.6.11"
        },
        "build": governed.pointer("/build").unwrap().clone(),
        "conclusion": "success",
        "governance": {
            "manifest_commit": GOVERNED_JANKURAI_MANIFEST_COMMIT,
            "manifest_repo": GOVERNED_JANKURAI_MANIFEST_REPO,
            "manifest_sha256": GOVERNED_JANKURAI_MANIFEST_SHA256,
            "manifest_tree": GOVERNED_JANKURAI_MANIFEST_TREE,
            "protected_main": true,
            "protection_policy": "immutable-main-v1",
            "status": "governed"
        },
        "installation": {
            "atomic": true,
            "path": binary
        },
        "operator": "jeryu-verifier-test",
        "run_id": "jeryu-verifier-test-run",
        "schema": "jeryu.jankurai-installation/v2",
        "source": {
            "archive_sha256": GOVERNED_JANKURAI_SOURCE_ARCHIVE_SHA256,
            "cargo_lock_sha256": GOVERNED_JANKURAI_CARGO_LOCK_SHA256,
            "commit": GOVERNED_JANKURAI_SOURCE_REV,
            "remote": GOVERNED_JANKURAI_SOURCE_REPO,
            "tag": GOVERNED_JANKURAI_SOURCE_TAG,
            "tree": GOVERNED_JANKURAI_SOURCE_TREE,
            "verification": "release-authoritative"
        },
        "test_mode": false
    })
}

fn write_content_addressed_receipt(root: &Path, document: &serde_json::Value) -> PathBuf {
    let bytes = serde_json::to_vec_pretty(document).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    let path = root.join(format!("{digest}.json"));
    fs::write(&path, bytes).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn governed_jankurai_authority_requires_complete_content_addressed_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let binary = temp.path().join("jankurai");
    write_test_jankurai(&binary);
    let binary_sha = hex::encode(Sha256::digest(fs::read(&binary).unwrap()));

    let valid_document = governed_receipt(&binary, &binary_sha);
    let valid = write_content_addressed_receipt(temp.path(), &valid_document);
    let valid_result = verify_jankurai_authority(
        &binary,
        std::slice::from_ref(&valid),
        "jankurai 1.6.11",
        &binary_sha,
    );
    assert!(valid_result.is_ok(), "{valid_result:?}");
    assert!(verify_jankurai_authority(&binary, &[], "jankurai 1.6.11", &binary_sha).is_err());

    let tampered = write_content_addressed_receipt(temp.path(), &valid_document);
    fs::write(&tampered, b"{}").unwrap();
    assert!(
        verify_jankurai_authority(&binary, &[tampered], "jankurai 1.6.11", &binary_sha,).is_err()
    );

    let mut wrong_schema = valid_document.clone();
    wrong_schema["schema"] = serde_json::json!("jeryu.jankurai-installation/v1");
    let wrong_schema_path = write_content_addressed_receipt(temp.path(), &wrong_schema);
    assert!(
        verify_jankurai_authority(
            &binary,
            &[wrong_schema_path],
            "jankurai 1.6.11",
            &binary_sha,
        )
        .is_err()
    );

    let build_fields = valid_document["build"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for field in &build_fields {
        let mut wrong = valid_document.clone();
        wrong["build"][field.as_str()] = serde_json::json!("wrong-authority");
        let wrong_path = write_content_addressed_receipt(temp.path(), &wrong);
        assert!(
            verify_jankurai_authority(&binary, &[wrong_path], "jankurai 1.6.11", &binary_sha,)
                .is_err(),
            "build-field mutation was accepted: {field}"
        );

        let mut missing = valid_document.clone();
        missing["build"].as_object_mut().unwrap().remove(field);
        let missing_path = write_content_addressed_receipt(temp.path(), &missing);
        assert!(
            verify_jankurai_authority(&binary, &[missing_path], "jankurai 1.6.11", &binary_sha,)
                .is_err(),
            "missing build field was accepted: {field}"
        );
    }

    let mut unexpected_build_field = valid_document.clone();
    unexpected_build_field["build"]["unexpected_authority"] = serde_json::json!(true);
    let unexpected_build_path =
        write_content_addressed_receipt(temp.path(), &unexpected_build_field);
    assert!(
        verify_jankurai_authority(
            &binary,
            &[unexpected_build_path],
            "jankurai 1.6.11",
            &binary_sha,
        )
        .is_err()
    );

    let mut installed_document = valid_document.clone();
    installed_document["timestamp"] = serde_json::json!("2026-08-12T00:00:00Z");
    installed_document["installation"]["previous_binary_sha256"] =
        serde_json::json!("0".repeat(64));
    installed_document["installation"]["rollback_artifact"] =
        serde_json::json!("/tmp/jankurai-rollback");
    installed_document["installation"]["lock"] = serde_json::json!({
        "exclusive": true,
        "held_through_receipt": true,
        "identity": "1:2:3:4:600:1",
        "path": "/tmp/jankurai-install.lock"
    });
    let installed_path = write_content_addressed_receipt(temp.path(), &installed_document);
    assert!(
        verify_jankurai_authority(&binary, &[installed_path], "jankurai 1.6.11", &binary_sha,)
            .is_ok()
    );

    for (pointer, field) in [
        ("", "unexpected_authority"),
        ("/source", "unexpected_authority"),
        ("/governance", "unexpected_authority"),
        ("/binary", "unexpected_authority"),
    ] {
        let mut unexpected = valid_document.clone();
        unexpected
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(field.to_string(), serde_json::json!(true));
        let unexpected_path = write_content_addressed_receipt(temp.path(), &unexpected);
        assert!(
            verify_jankurai_authority(&binary, &[unexpected_path], "jankurai 1.6.11", &binary_sha,)
                .is_err(),
            "unexpected authority field was accepted: {pointer}/{field}"
        );
    }

    let mut missing_operator = valid_document.clone();
    missing_operator.as_object_mut().unwrap().remove("operator");
    let missing_operator_path = write_content_addressed_receipt(temp.path(), &missing_operator);
    assert!(
        verify_jankurai_authority(
            &binary,
            &[missing_operator_path],
            "jankurai 1.6.11",
            &binary_sha,
        )
        .is_err()
    );

    let mut wrong_run_id = valid_document.clone();
    wrong_run_id["run_id"] = serde_json::json!(42);
    let wrong_run_id_path = write_content_addressed_receipt(temp.path(), &wrong_run_id);
    assert!(
        verify_jankurai_authority(
            &binary,
            &[wrong_run_id_path],
            "jankurai 1.6.11",
            &binary_sha,
        )
        .is_err()
    );

    let mut empty_timestamp = installed_document;
    empty_timestamp["timestamp"] = serde_json::json!("");
    let empty_timestamp_path = write_content_addressed_receipt(temp.path(), &empty_timestamp);
    assert!(
        verify_jankurai_authority(
            &binary,
            &[empty_timestamp_path],
            "jankurai 1.6.11",
            &binary_sha,
        )
        .is_err()
    );

    for pointer in [
        "/source/remote",
        "/source/tag",
        "/source/commit",
        "/source/tree",
        "/source/archive_sha256",
        "/source/cargo_lock_sha256",
        "/governance/manifest_commit",
        "/governance/manifest_tree",
        "/governance/manifest_sha256",
        "/binary/sha256",
        "/installation/path",
    ] {
        let mut wrong = valid_document.clone();
        *wrong.pointer_mut(pointer).unwrap() = serde_json::json!("wrong-authority");
        let wrong_path = write_content_addressed_receipt(temp.path(), &wrong);
        assert!(
            verify_jankurai_authority(&binary, &[wrong_path], "jankurai 1.6.11", &binary_sha,)
                .is_err(),
            "receipt mutation was accepted: {pointer}"
        );
    }
}

fn init_version_repo(root: &Path) -> (String, String) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "ci@example.invalid"]);
    git(root, &["config", "user.name", "CI"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/demo\"]\n\n[workspace.package]\nversion = \"4.0.0\"\nedition = \"2024\"\nlicense = \"Apache-2.0\"\nrust-version = \"1.95\"\n",
    );
    write(
        root,
        "crates/demo/Cargo.toml",
        "[package]\nname = \"demo\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\nrust-version.workspace = true\n\n[lib]\npath = \"src/lib.rs\"\n",
    );
    write(root, "crates/demo/src/lib.rs", "pub fn demo() {}\n");
    write(
        root,
        "CHANGELOG.md",
        "# Changelog\n\n## Unreleased\n\n- seed\n",
    );
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "chore: base"]);
    let base = git_out(root, &["rev-parse", "HEAD"]);

    write(root, "docs/feature.md", "feature\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "feat: add dashboard signal"]);
    let head = git_out(root, &["rev-parse", "HEAD"]);
    (base, head)
}

fn clone_bare(work: &Path, bare: &Path) {
    let bare_str = bare.to_string_lossy().to_string();
    git(work, &["clone", "--bare", "--no-local", ".", &bare_str]);
}

fn install_main_blocking_hook(bare: &Path) {
    let hook = bare.join("hooks").join("pre-receive");
    fs::create_dir_all(hook.parent().expect("hook parent")).unwrap();
    fs::write(
        &hook,
        "#!/usr/bin/env bash\nset -euo pipefail\nwhile read -r _old _new ref; do\n  if [[ \"$ref\" == \"refs/heads/main\" ]]; then\n    echo 'direct main push blocked by test hook' >&2\n    exit 1\n  fi\ndone\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn ref_update(ref_name: &str, previous_oid: &str, new_oid: &str) -> RefUpdate {
    RefUpdate {
        ref_name: ref_name.to_owned(),
        old_oid: previous_oid.to_owned(),
        new_oid: new_oid.to_owned(),
    }
}

#[cfg(unix)]
#[test]
fn push_audit_replaces_a_preexisting_tool_failure_at_the_same_head() {
    use std::os::unix::fs::PermissionsExt;

    let work = tempfile::tempdir().unwrap();
    let bare = tempfile::tempdir().unwrap();
    let tool = tempfile::tempdir().unwrap();
    let (_, head) = init_version_repo(work.path());
    clone_bare(work.path(), bare.path());

    let auditor = tool.path().join("jankurai");
    let script = format!(
        r#"#!/bin/sh
set -eu
output=
base=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --json)
      output=$2
      shift 2
      ;;
    --base-ref)
      base=$2
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
test -n "$output"
test -n "$base"
test "$base" != "{}"
mkdir -p "$(dirname "$output")"
printf '%s\n' '{{"score":92,"caps_applied":[],"decision":{{"hard_findings":0,"minimum_score":85}}}}' > "$output"
"#,
        head
    );
    fs::write(&auditor, script).unwrap();
    fs::set_permissions(&auditor, fs::Permissions::from_mode(0o755)).unwrap();

    let core = ForgeCore::new();
    core.create_repository(
        "jeryu",
        CreateRepositoryRequest {
            name: "demo".to_string(),
            private: true,
            description: None,
            default_branch: Some("main".to_string()),
        },
    )
    .unwrap();
    core.record_jankurai_score(
        "jeryu",
        "demo",
        RecordJankuraiScoreRequest {
            branch: "main".to_string(),
            commit_sha: head.clone(),
            decision: "tool-failed".to_string(),
            tool_exit: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    core.create_check_run(
        "jeryu",
        "demo",
        CreateCheckRunRequest {
            name: "jankurai/proof".to_string(),
            head_sha: head.clone(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(CheckConclusion::Failure),
            ..Default::default()
        },
    )
    .unwrap();

    record_authoritative_jankurai_score_with(
        &core,
        "git",
        bare.path(),
        "jeryu",
        "demo",
        &ref_update("refs/heads/main", ZERO_OID, &head),
        || Ok(auditor),
    );

    let scores = core
        .list_jankurai_scores("jeryu", "demo", Some("main"), Some(&head))
        .unwrap();
    assert_eq!(scores.len(), 1, "same-head recovery must remain an upsert");
    assert_eq!(scores[0].decision, "scored");
    assert_eq!(scores[0].score, Some(92));
    assert_eq!(scores[0].hard_findings, 0);
    assert!(scores[0].caps_applied.is_empty());

    let checks = core.list_check_runs("jeryu", "demo", Some(&head)).unwrap();
    assert!(
        checks.check_runs.iter().any(|check| {
            check.name == "jankurai/proof"
                && check.status == CheckRunStatus::Completed
                && check.conclusion == Some(CheckConclusion::Success)
        }),
        "the recomputed score must publish a succeeding exact-head proof"
    );
    let latest = checks
        .check_runs
        .iter()
        .max_by_key(|check| check.completed_at.unwrap_or(check.started_at))
        .unwrap();
    assert_eq!(latest.name, "jankurai/proof");
    assert_eq!(latest.conclusion, Some(CheckConclusion::Success));
}

#[test]
fn malformed_or_nonzero_jankurai_reports_are_never_green() {
    let valid = serde_json::json!({
        "score": 92,
        "caps_applied": [],
        "decision": {"hard_findings": 0, "minimum_score": 85}
    });
    let (request, pass) = jankurai_score_request("main", "abc", Some(valid.clone()), 0);
    assert!(pass);
    assert_eq!(request.decision, "scored");

    let hostile_reports = [
        (valid, 9),
        (
            serde_json::json!({
                "score": 92,
                "caps_applied": [],
                "decision": {"minimum_score": 85}
            }),
            0,
        ),
        (
            serde_json::json!({
                "score": 101,
                "caps_applied": [],
                "decision": {"hard_findings": 0, "minimum_score": 85}
            }),
            0,
        ),
        (
            serde_json::json!({
                "score": 92,
                "caps_applied": [7],
                "decision": {"hard_findings": 0, "minimum_score": 85}
            }),
            0,
        ),
    ];
    for (report, exit_code) in hostile_reports {
        let (request, pass) = jankurai_score_request("main", "abc", Some(report), exit_code);
        assert!(!pass);
        assert_eq!(request.decision, "tool-failed");
        assert_eq!(request.score, None);
    }

    let (request, pass) = jankurai_score_request(
        "main",
        "abc",
        Some(serde_json::json!({
            "score": 80,
            "caps_applied": [],
            "decision": {"hard_findings": 0, "minimum_score": 85}
        })),
        0,
    );
    assert!(!pass);
    assert_eq!(
        request.decision, "scored",
        "a valid red audit is not a tool error"
    );
    assert_eq!(request.score, Some(80));

    let (request, pass) = jankurai_score_request(
        "main",
        "abc",
        Some(serde_json::json!({
            "score": 80,
            "caps_applied": [],
            "decision": {"hard_findings": 0, "minimum_score": 0}
        })),
        0,
    );
    assert!(!pass, "candidate policy cannot lower the host score floor");
    assert_eq!(request.decision, "scored");
}

#[test]
fn branch_head_skips_tag_only_release_workflow() {
    let workflow = r#"
name: release
on:
  push:
    tags: ['v*']
  workflow_dispatch:
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - run: bash ops/ci/release.sh
"#;

    assert!(!workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn branch_head_runs_pull_request_workflow_even_with_main_push_filter() {
    let workflow = r#"
name: web
on:
  push:
    branches:
      - main
  pull_request:
jobs:
  web:
    runs-on: ubuntu-latest
    steps:
      - run: bash ops/ci/web.sh
"#;

    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn branch_push_filter_matches_only_named_branches() {
    let workflow = r#"
name: branch-only
on:
  push:
    branches: [main, release/*]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: true
"#;

    assert!(workflow_runs_for_branch_head(workflow, "refs/heads/main"));
    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/release/next"
    ));
    assert!(!workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn inline_on_list_runs_for_branch_push() {
    let workflow = r#"
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: true
"#;

    assert!(workflow_runs_for_branch_head(
        workflow,
        "refs/heads/codex/feature"
    ));
}

#[test]
fn workflow_toolchain_bootstrap_step_is_skipped_locally() {
    assert!(is_workflow_toolchain_bootstrap(
        "rustup toolchain install 1.95.0 --profile minimal"
    ));
    assert!(!is_workflow_toolchain_bootstrap(
        "rustup toolchain install 1.95.0 --profile minimal\ncargo test"
    ));
    assert!(!is_workflow_toolchain_bootstrap("bash ops/ci/ci-fast.sh"));
}

#[test]
fn ref_updates_track_ref_name_and_previous_oid() {
    let before = vec![
        GitRef {
            name: "refs/heads/main".to_owned(),
            oid: "aaa".to_owned(),
        },
        GitRef {
            name: "refs/heads/feature".to_owned(),
            oid: "bbb".to_owned(),
        },
    ];
    let after = vec![
        GitRef {
            name: "refs/heads/main".to_owned(),
            oid: "ccc".to_owned(),
        },
        GitRef {
            name: "refs/heads/feature".to_owned(),
            oid: "bbb".to_owned(),
        },
    ];

    let updates = ref_updates(&before, &after);

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].ref_name, "refs/heads/main");
    assert_eq!(updates[0].old_oid, "aaa");
    assert_eq!(updates[0].new_oid, "ccc");
}

fn assert_reviewed_main_preserved(explicit_version: bool) {
    use jeryu_core::{CreateReviewRequest, ReviewState};
    use jeryu_gitd::GitdConfig;
    use std::sync::Arc;

    let work = tempfile::tempdir().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let (base, mut head) = init_version_repo(work.path());
    if explicit_version {
        let manifest = fs::read_to_string(work.path().join("Cargo.toml")).unwrap();
        write(
            work.path(),
            "Cargo.toml",
            &manifest.replace("4.0.0", "4.1.0"),
        );
        write(
            work.path(),
            "CHANGELOG.md",
            "# Changelog\n\n## v4.1.0\n\n- Reviewed feature.\n",
        );
        git(work.path(), &["add", "Cargo.toml", "CHANGELOG.md"]);
        git(
            work.path(),
            &["commit", "-q", "-m", "chore(release): prepare v4.1.0"],
        );
        head = git_out(work.path(), &["rev-parse", "HEAD"]);
    }
    // Neither candidate uses the legacy recursion marker: preservation must
    // follow from the bridge's behavior, including an explicit version update.
    assert!(!git_out(work.path(), &["log", "-1", "--format=%s"]).contains("[skip-version]"));
    git(work.path(), &["tag", "demo-v4.0.0-split.0", &base]);
    let manager = Arc::new(RepoManager::new(GitdConfig::new(storage.path())));
    let bare = storage.path().join("jeryu/demo.git");
    fs::create_dir_all(bare.parent().unwrap()).unwrap();
    clone_bare(work.path(), &bare);
    git(&bare, &["update-ref", "refs/heads/feature", &head]);
    git(&bare, &["update-ref", "refs/heads/main", &base, &head]);
    install_main_blocking_hook(&bare);
    let direct = Command::new("git")
        .current_dir(work.path())
        .args(["push", bare.to_str().unwrap(), "HEAD:refs/heads/main"])
        .output()
        .unwrap();
    assert!(
        !direct.status.success(),
        "direct main push must be rejected"
    );
    assert!(String::from_utf8_lossy(&direct.stderr).contains("direct main push blocked"));

    let router =
        crate::GithubRouter::with_core(ForgeCore::new()).with_repo_manager(manager.clone());
    let created = router.post(
        "/repos",
        r#"{"owner":"jeryu","name":"demo","default_branch":"main"}"#,
    );
    assert_eq!(created.status, 201, "{}", created.body);
    let opened = router.post(
        "/repos/jeryu/demo/pulls",
        &serde_json::json!({
            "title": "Reviewed feature", "head": "feature", "base": "main",
            "head_sha": head, "base_sha": base, "actor": "author"
        })
        .to_string(),
    );
    assert_eq!(opened.status, 201, "{}", opened.body);
    let number = serde_json::from_str::<serde_json::Value>(&opened.body).unwrap()["number"]
        .as_u64()
        .unwrap();
    let protected = router.put("/repos/jeryu/demo/branches/main/protection",
        r#"{"required_approving_review_count":1,"required_status_checks":["demo/required"],"enforce_admins":true,"required_linear_history":true}"#);
    assert_eq!(protected.status, 200, "{}", protected.body);
    let policy: serde_json::Value = serde_json::from_str(&protected.body).unwrap();
    assert_eq!(
        policy["required_pull_request_reviews"]["required_approving_review_count"],
        1
    );
    assert_eq!(
        policy["required_status_checks"]["contexts"],
        serde_json::json!(["demo/required"])
    );
    assert_eq!(policy["enforce_admins"]["enabled"], true);
    assert_eq!(policy["required_linear_history"]["enabled"], true);
    let merge_path = format!("/repos/jeryu/demo/pulls/{number}/merge");
    let unapproved = router.put(&merge_path, "{}");
    assert_ne!(
        unapproved.status, 200,
        "unapproved candidate must be blocked"
    );
    assert_eq!(git_out(&bare, &["rev-parse", "refs/heads/main"]), base);
    router
        .core()
        .create_review(
            "jeryu",
            "demo",
            number,
            "reviewer",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: vec![],
                expected_head_sha: Some(head.clone()),
            },
        )
        .unwrap();
    let unchecked = router.put(&merge_path, "{}");
    assert_ne!(
        unchecked.status, 200,
        "candidate without its required check must be blocked"
    );
    assert_eq!(git_out(&bare, &["rev-parse", "refs/heads/main"]), base);
    // In-process fixture evidence exercises the real protection decision; it is
    // never published to a forge or reported as a production required check.
    router
        .core()
        .create_check_run(
            "jeryu",
            "demo",
            CreateCheckRunRequest {
                name: "demo/required".to_string(),
                head_sha: head.clone(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                ..Default::default()
            },
        )
        .unwrap();
    let merged = router.put(&merge_path, "{}");
    assert_eq!(merged.status, 503, "{}", merged.body);
    assert_eq!(git_out(&bare, &["rev-parse", "refs/heads/main"]), base);
    let final_pr = router.get(&format!("/repos/jeryu/demo/pulls/{number}"));
    assert_eq!(final_pr.status, 200, "{}", final_pr.body);
    let final_pr: serde_json::Value = serde_json::from_str(&final_pr.body).unwrap();
    assert_eq!(final_pr["merged"], false);
    assert_eq!(
        git_out(&bare, &["rev-parse", "refs/tags/demo-v4.0.0-split.0"]),
        base
    );
}

#[test]
fn reviewed_main_feature_stays_at_exact_approved_head() {
    assert_reviewed_main_preserved(false);
}

#[test]
fn reviewed_main_explicit_version_stays_at_exact_approved_head() {
    assert_reviewed_main_preserved(true);
}

#[test]
fn mock_flag_gates_workflow_check_run_seeding() {
    // The production forge (no JERYU_CI_MOCK) must NOT seed GitHub Actions check-runs
    // — it has no Actions runners, so they only produced all-red noise; host-ci's
    // `jeryu/ci` is the real gate. Only the in-process CI-seeding-flow tests opt in.
    // Pure predicate, so this never mutates the shared process env (which would race
    // the parallel seeded-CI tests that read JERYU_CI_MOCK).
    assert!(
        !mock_flag_set(None),
        "unset -> production posture, no seeding"
    );
    assert!(!mock_flag_set(Some("")));
    assert!(!mock_flag_set(Some("0")));
    assert!(!mock_flag_set(Some("  0  ")));
    assert!(mock_flag_set(Some("1")), "opt-in for tests");
    assert!(mock_flag_set(Some("true")));
}
