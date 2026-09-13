//! Actual Git/source admission for Runner's exported auditor dependency.
//! Synthetic inputs never supply a successful auditor or installation receipt.
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn component() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn source_library() -> PathBuf {
    let component = component();
    let root = component
        .ancestors()
        .find(|path| path.join("scripts/source-build.sh").is_file())
        .unwrap();
    root.join("scripts/source-build.sh")
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Synthetic Runner fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Synthetic Runner fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .args(["-c", "core.hooksPath=/dev/null", "-C"])
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn write(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn commit(root: &Path) -> String {
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "synthetic source"]);
    git(root, &["rev-parse", "HEAD"])
}

struct Fixture {
    root: PathBuf,
    export: PathBuf,
    source: PathBuf,
    descriptor: Value,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "jeryu-runner-auditor-test-{}-{nonce}",
            std::process::id()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        // Preserve every specimen, including failed assertions, in private custody.
        eprintln!(
            "retained synthetic Runner auditor fixture: {}",
            root.display()
        );
        let source = root.join("source");
        let export = root.join("export");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&export).unwrap();
        for path in [
            "ops/ci/lib.sh",
            "ops/ci/public-auditor.sh",
            "agent/audit-policy.toml",
        ] {
            let bytes = format!("synthetic consumer input: {path}\n");
            write(
                &source,
                &format!("components/jeryu-ci-runner/{path}"),
                bytes.as_bytes(),
            );
            write(&export, path, bytes.as_bytes());
        }
        for path in ["scripts/source-build.sh", "rust-toolchain.toml"] {
            write(&source, path, b"synthetic common input\n");
            write(&export, path, b"synthetic common input\n");
        }
        git(&source, &["init", "--quiet", "-b", "main"]);
        let head = commit(&source);
        let tree = git(&source, &["rev-parse", "HEAD:components/jeryu-ci-runner"]);
        let descriptor = json!({"schema_version":"jeryu.split-provenance/v1", "component":"jeryu-ci-runner",
            "source_commit":head,"original_component_tree":tree,"lock_regeneration_required":false});
        write(
            &export,
            ".jeryu-source.json",
            &serde_json::to_vec(&descriptor).unwrap(),
        );
        git(&export, &["init", "--quiet", "-b", "main"]);
        commit(&export);
        Self {
            root,
            export,
            source,
            descriptor,
        }
    }

    fn run(&self, body: &str, source: &Path) -> Output {
        Command::new("/bin/bash")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(["-euo", "pipefail", "-c", body, "synthetic-runner-admission"])
            .arg(component().join("ops/ci/public-auditor.sh"))
            .arg(source_library())
            .arg(&self.export)
            .arg(source)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn binding(&self, source: &Path) -> Output {
        self.run(
            r#"source "$1"; source "$2"
runner_public_root=$3
selected=$(runner_public_descriptor "$3")
IFS=$'\t' read -r runner_public_revision runner_public_subtree <<< "$selected"
runner_public_source_binding "$4""#,
            source,
        )
    }

    fn set_descriptor(&mut self, field: &str, value: Value) {
        self.descriptor[field] = value;
        write(
            &self.export,
            ".jeryu-source.json",
            &serde_json::to_vec(&self.descriptor).unwrap(),
        );
    }
}

#[test]
fn exact_source_and_consumer_binding_refuses_drift_and_aliases() {
    let fixture = Fixture::new();
    let accepted = fixture.binding(&fixture.source);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(
        !fixture
            .binding(&fixture.root.join("missing"))
            .status
            .success()
    );
    let alias = fixture.root.join("linked-source");
    symlink(&fixture.source, &alias).unwrap();
    assert!(!fixture.binding(&alias).status.success());
    write(
        &fixture.export,
        "agent/audit-policy.toml",
        b"different policy\n",
    );
    commit(&fixture.export);
    assert!(!fixture.binding(&fixture.source).status.success());
}

#[test]
fn source_binding_rejects_changed_source_even_with_hidden_index_flags() {
    let fixture = Fixture::new();
    assert!(fixture.binding(&fixture.source).status.success());
    git(
        &fixture.source,
        &["update-index", "--assume-unchanged", "rust-toolchain.toml"],
    );
    write(
        &fixture.source,
        "rust-toolchain.toml",
        b"unreviewed compiler\n",
    );
    assert!(!fixture.binding(&fixture.source).status.success());
}

#[test]
fn descriptor_and_exact_revision_mismatches_fail_before_tool_execution() {
    for (field, value) in [
        ("schema_version", json!("unknown")),
        ("component", json!("jeryu-core")),
        ("lock_regeneration_required", json!(true)),
        ("source_commit", json!(42)),
        ("source_commit", json!("a".repeat(40))),
        ("original_component_tree", json!("b".repeat(40))),
    ] {
        let mut fixture = Fixture::new();
        assert!(fixture.binding(&fixture.source).status.success());
        fixture.set_descriptor(field, value);
        assert!(
            !fixture.binding(&fixture.source).status.success(),
            "{field}"
        );
    }
    let fixture = Fixture::new();
    let descriptor = fs::read_to_string(fixture.export.join(".jeryu-source.json")).unwrap();
    write(
        &fixture.export,
        ".jeryu-source.json",
        format!("{descriptor}\n{descriptor}").as_bytes(),
    );
    assert!(!fixture.binding(&fixture.source).status.success());
}
