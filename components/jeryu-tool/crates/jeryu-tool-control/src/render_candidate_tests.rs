use super::super::tests::{run_fixture_git, test_root};
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

struct Fixture {
    root: PathBuf,
    tool: PathBuf,
    root_identity: (u64, u64),
}

impl Fixture {
    fn new() -> Self {
        let root = test_root("candidate");
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("private fixture");
        let metadata = fs::symlink_metadata(&root).expect("root identity");
        let tool = root.join(TOOL_DIRECTORY);
        fs::create_dir_all(tool.join("generated")).expect("fixture pin directory");
        fs::create_dir_all(tool.join("ops/render-assets")).expect("fixture template directory");
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for name in [
            "tool-manifest.toml",
            "generated/jankurai-pin.env",
            "ops/render-assets/require-jankurai.sh",
        ] {
            fs::write(
                tool.join(name),
                fs::read(original.join(name)).expect("read fixture input"),
            )
            .expect("fixture input");
        }
        for component in ["jeryu-tool", "jeryu-intelligence"] {
            let lib = root
                .join("components")
                .join(component)
                .join("ops/ci/lib.sh");
            fs::create_dir_all(lib.parent().expect("library parent"))
                .expect("fixture library directory");
            let template = fs::read_to_string(tool.join("ops/render-assets/require-jankurai.sh"))
                .expect("fixture template");
            fs::write(
                lib,
                format!("#!/usr/bin/env bash\nset -euo pipefail\n{template}"),
            )
            .expect("fixture CI library");
        }
        let consumer = root.join("components/jeryu-intelligence/ops/ci/ensure-jankurai.sh");
        fs::create_dir_all(consumer.parent().expect("consumer parent"))
            .expect("fixture consumer directory");
        let pin = Pin::load(&tool).expect("fixture pin");
        fs::write(
            &consumer,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\n{}\n",
                pin.shell_block().replace(pin.get("rev"), &"0".repeat(40))
            ),
        )
        .expect("stale generated consumer");
        let receipt = root.join(
            "components/jeryu-intelligence/images/agent-sandbox/jankurai-installation-receipt.json",
        );
        fs::create_dir_all(receipt.parent().expect("receipt parent"))
            .expect("fixture historical directory");
        fs::write(receipt, "historical receipt fixture: unchanged\n").expect("historical fixture");
        run_fixture_git(&root, &["init", "-q"]);
        run_fixture_git(&root, &["config", "user.name", "Jeryu Test"]);
        run_fixture_git(&root, &["config", "user.email", "jeryu-test@invalid"]);
        run_fixture_git(&root, &["add", "."]);
        run_fixture_git(&root, &["commit", "-qm", "candidate fixture"]);
        Self {
            root,
            tool,
            root_identity: (metadata.dev(), metadata.ino()),
        }
    }

    fn head(&self) -> String {
        git_local_output(&self.root, &["rev-parse", "HEAD"]).expect("fixture HEAD")
    }

    fn args(&self, mode: &str) -> Vec<String> {
        [
            "--monorepo-root".to_owned(),
            self.root.to_str().expect("fixture path").to_owned(),
            mode.to_owned(),
            "--expected-head".to_owned(),
            self.head(),
            "--repo".to_owned(),
            "jeryu-tool".to_owned(),
            "--repo".to_owned(),
            "jeryu-intelligence".to_owned(),
        ]
        .to_vec()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let metadata = fs::symlink_metadata(&self.root).expect("fixture root readback");
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || (metadata.dev(), metadata.ino()) != self.root_identity
            || fs::canonicalize(&self.root).ok().as_ref() != Some(&self.root)
        {
            eprintln!(
                "retaining changed candidate fixture: {}",
                self.root.display()
            );
            return;
        }
        let mounts = fs::read_to_string("/proc/self/mountinfo").expect("fixture mount readback");
        if mounts
            .lines()
            .filter_map(|line| line.split_whitespace().nth(4))
            .any(|mount| Path::new(mount).starts_with(&self.root))
        {
            eprintln!(
                "retaining mounted candidate fixture: {}",
                self.root.display()
            );
            return;
        }
        fn links_are_owned(root: &Path, directory: &Path) -> bool {
            fs::read_dir(directory)
                .expect("fixture entries")
                .all(|entry| {
                    let entry = entry.expect("fixture entry");
                    let kind = entry.file_type().expect("fixture node type");
                    if kind.is_symlink() {
                        fs::read_link(entry.path()).is_ok_and(|target| target.starts_with(root))
                    } else if kind.is_dir() {
                        links_are_owned(root, &entry.path())
                    } else {
                        kind.is_file()
                    }
                })
        }
        if !links_are_owned(&self.root, &self.root) {
            eprintln!(
                "retaining unexpected candidate fixture links: {}",
                self.root.display()
            );
            return;
        }
        fs::remove_dir_all(&self.root).expect("remove inspected fixture without following links");
    }
}

#[test]
fn candidate_arguments_and_generated_output_scope_are_closed() {
    for arguments in [
        vec![],
        vec!["--monorepo-root", "relative"],
        vec!["--monorepo-root", "/fixture", "--write"],
        vec!["--monorepo-root", "/fixture", "--check", "--write"],
        vec!["--monorepo-root", "/fixture", "--repo", "../jeryu-tool"],
        vec![
            "--monorepo-root",
            "/fixture",
            "--family-root",
            "/caller-authority",
        ],
        vec!["--monorepo-root", "/fixture", "--expected-head", "HEAD"],
    ] {
        assert!(
            candidate_args(
                &arguments
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            )
            .is_err()
        );
    }
    let check = [
        "--monorepo-root",
        "/fixture",
        "--check",
        "--expected-head",
        &"a".repeat(40),
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect::<Vec<_>>();
    assert!(!candidate_args(&check).expect("exact check head").write);
    for excluded in [
        "images/agent-sandbox/jankurai-installation-receipt.json",
        "images/agent-sandbox/Dockerfile",
        "ops/ci/test-governed-jankurai.sh",
        "crates/jeryu-api/src/ci_bridge.rs",
        "agent/native-cli-manifest.toml",
        "docs/release.md",
        ".github/workflows/nested/ci.yml",
    ] {
        assert!(
            !candidate_path(Path::new(excluded)),
            "unexpected candidate output: {excluded}"
        );
    }
}

#[test]
fn candidate_rendering_updates_consumers_and_emits_unendorsed_deterministic_provenance() {
    let fixture = Fixture::new();
    let pin_before = fs::read(fixture.root.join(PIN_PATH)).expect("original pin bytes");
    let (code, before) =
        execute(&fixture.tool, &fixture.args("--check")).expect("candidate drift report");
    assert_eq!(code, 1);
    assert!(before.starts_with("{\n  \"consumers\": "));
    assert!(before.contains("\"source\": {\n    \"clean_at_start\": true,\n    \"commit\": "));
    let report: serde_json::Value = serde_json::from_str(&before).expect("candidate JSON");
    assert_eq!(report["schema"], "jeryu.jankurai-candidate-render/v1");
    assert_eq!(report["scope"]["complete"], false);
    assert_eq!(
        report["scope"]["repositories"],
        serde_json::json!(["jeryu-intelligence", "jeryu-tool"])
    );
    assert_eq!(report["governance"]["protected_main"], false);
    assert_eq!(report["governance"]["handover"], "pending");
    assert_eq!(
        report["governance"]["predecessor_authentication"],
        "not-performed"
    );
    assert_eq!(report["verification"]["build"], "not-performed");
    assert_eq!(report["verification"]["installation"], "not-performed");
    assert!(
        report["consumers"]
            .as_array()
            .expect("consumers")
            .iter()
            .any(|item| item["changed"] == true)
    );
    assert!(
        git_local_output(&fixture.root, &["status", "--porcelain"])
            .expect("clean fixture")
            .is_empty()
    );
    let (code, written) =
        execute(&fixture.tool, &fixture.args("--write")).expect("candidate write");
    assert_eq!(code, 0);
    let written: serde_json::Value = serde_json::from_str(&written).expect("write provenance");
    assert_eq!(written["source"], report["source"]);
    assert_eq!(written["mode"], "write");
    assert!(
        execute(&fixture.tool, &fixture.args("--check"))
            .expect_err("uncommitted output rejected")
            .contains("must be clean")
    );
    assert_eq!(
        fs::read(fixture.root.join(PIN_PATH)).expect("unchanged pin"),
        pin_before
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join(
            "components/jeryu-intelligence/images/agent-sandbox/jankurai-installation-receipt.json"
        ))
        .expect("historical receipt"),
        "historical receipt fixture: unchanged\n"
    );
    run_fixture_git(&fixture.root, &["add", "."]);
    run_fixture_git(
        &fixture.root,
        &["commit", "-qm", "generated candidate consumers"],
    );
    let (code, after) =
        execute(&fixture.tool, &fixture.args("--check")).expect("candidate current");
    assert_eq!(code, 0);
    assert!(after.ends_with('\n') && !after.ends_with("\n\n"));
    assert_eq!(
        after,
        execute(&fixture.tool, &fixture.args("--check"))
            .expect("repeat provenance")
            .1
    );
    let after: serde_json::Value = serde_json::from_str(&after).expect("current provenance");
    assert_eq!(after["drift"], false);
    assert_eq!(after["source"]["commit"], fixture.head());
    assert!(
        after["consumers"]
            .as_array()
            .expect("consumers")
            .iter()
            .all(
                |item| item["changed"] == false && item["before_sha256"] == item["expected_sha256"]
            )
    );
}

#[test]
fn candidate_source_rejects_wrong_heads_hidden_index_changes_and_linked_outputs() {
    let fixture = Fixture::new();
    let mut wrong_head = fixture.args("--write");
    wrong_head[4] = "0".repeat(40);
    assert!(
        execute(&fixture.tool, &wrong_head)
            .expect_err("wrong source")
            .contains("HEAD mismatch")
    );

    let alias = fixture.root.join("source-alias");
    symlink(&fixture.root, &alias).expect("source alias");
    assert!(
        snapshot(&alias, None, true)
            .expect_err("source alias")
            .contains("physical absolute")
    );
    fs::remove_file(alias).expect("remove inspected fixture alias");

    run_fixture_git(
        &fixture.root,
        &["update-index", "--assume-unchanged", MANIFEST_PATH],
    );
    assert!(
        execute(&fixture.tool, &fixture.args("--check"))
            .expect_err("hidden index state")
            .contains("suppression")
    );
    run_fixture_git(
        &fixture.root,
        &["update-index", "--no-assume-unchanged", MANIFEST_PATH],
    );

    let output = fixture
        .root
        .join("components/jeryu-intelligence/ops/ci/ensure-jankurai.sh");
    let linked = fixture.root.join("linked-output.sh");
    fs::hard_link(&output, &linked).expect("linked output");
    run_fixture_git(&fixture.root, &["add", "."]);
    run_fixture_git(
        &fixture.root,
        &["commit", "-qm", "hostile output link fixture"],
    );
    assert!(
        execute(&fixture.tool, &fixture.args("--write"))
            .expect_err("hardlinked output")
            .contains("stable regular HEAD blob")
    );
}

#[test]
fn candidate_input_rejects_working_bytes_selected_by_replacement_refs() {
    const CHILD: &str = "JERYU_TOOL_REPLACEMENT_REF_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "render::candidate::tests::candidate_input_rejects_working_bytes_selected_by_replacement_refs",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env_remove("GIT_NO_REPLACE_OBJECTS")
            .env_remove("GIT_REPLACE_REF_BASE")
            .output()
            .expect("isolated replacement-ref test");
        assert!(
            output.status.success(),
            "isolated replacement-ref test failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("test output");
        let summaries: Vec<_> = stdout
            .lines()
            .filter(|line| line.starts_with("test result:"))
            .collect();
        assert_eq!(summaries.len(), 1, "exactly one child test must run");
        assert!(
            summaries[0].starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;")
        );
        return;
    }
    let fixture = Fixture::new();
    let original_head = fixture.head();
    let manifest_path = fixture.root.join(MANIFEST_PATH);
    let mut modified = fs::read_to_string(&manifest_path).expect("original fixture manifest");
    modified.push_str("\n# replacement-ref fixture\n");
    fs::write(&manifest_path, &modified).expect("modified fixture manifest");
    run_fixture_git(&fixture.root, &["add", "."]);
    run_fixture_git(
        &fixture.root,
        &["commit", "-qm", "replacement input fixture"],
    );
    let replacement_head = fixture.head();
    run_fixture_git(&fixture.root, &["reset", "--soft", &original_head]);
    run_fixture_git(
        &fixture.root,
        &["replace", &original_head, &replacement_head],
    );

    // Select the replacement-aware control explicitly, independent of CI's
    // Git environment. Candidate reads below must still use the original object.
    let replacement_view = |arguments: &[&str]| {
        let output = local_git_command(&fixture.root)
            .env_remove("GIT_NO_REPLACE_OBJECTS")
            .env_remove("GIT_REPLACE_REF_BASE")
            .args(arguments)
            .output()
            .expect("replacement-aware fixture Git");
        assert!(output.status.success(), "replacement-aware Git failed");
        output.stdout
    };
    assert!(replacement_view(&["status", "--porcelain"]).is_empty());
    assert_eq!(
        replacement_view(&["show", &format!("HEAD:{MANIFEST_PATH}")]),
        modified.into_bytes()
    );
    assert!(
        committed_text(&fixture.root, &original_head, MANIFEST_PATH)
            .expect_err("candidate input must match the unreplaced committed bytes")
            .contains("differs from its exact committed blob")
    );
}
