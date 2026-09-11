use std::process::{Command, Output};

const PROGRAM: &str = "/fixture/ops/split/manifest.sh";

fn run_manifest(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jeryu-split"))
        .env("JERYU_SPLIT_MANIFEST_PROGRAM", PROGRAM)
        .arg("manifest")
        .args(args)
        .output()
        .expect("run manifest compatibility entrypoint")
}

#[test]
fn missing_manifest_value_preserves_legacy_diagnostic_and_status_one() {
    let output = run_manifest(&["--manifest"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn empty_manifest_value_preserves_legacy_diagnostic_and_status_one() {
    let output = run_manifest(&["--manifest", ""]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"manifest error: --manifest requires a path\n"
    );
}

#[test]
fn unknown_manifest_argument_preserves_legacy_usage_and_status_two() {
    for argument in ["--unknown", "--help"] {
        let output = run_manifest(&[argument]);

        assert_eq!(output.status.code(), Some(2), "argument={argument}");
        assert!(output.stdout.is_empty(), "argument={argument}");
        assert_eq!(
            output.stderr,
            format!("usage: {PROGRAM} [--manifest PATH] [--json] [--check-paths]\n").as_bytes(),
            "argument={argument}"
        );
    }
}

#[test]
fn flag_consumed_as_manifest_path_preserves_legacy_unreadable_error() {
    let output = run_manifest(&["--manifest", "--json"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"manifest error: manifest not readable: --json\n"
    );
}

fn portal_manifest_fixtures() -> [(&'static str, toml::Value); 2] {
    use toml::Value;
    let historical: Value = toml::from_str(
        r#"
required_repos = ["jeryu"]
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
"#,
    )
    .unwrap();
    let mut candidate: Value = toml::from_str(
        r#"
schema_version = "jeryu.monorepo/v1"
required_repos = ["jeryu"]
repo_family = "jeryu-split"
release_lineage = "v5"
status = "candidate"
formal_ga = false
[handover]
status = "pending-protected-review"
[storage]
default_backend = "sqlite"
bundled_sqlite = true
[redline]
role = "optional-compatibility-proof"
required_for_release = false
contract_manifest = "components/jeryu-release-ops/tests/redline/Cargo.toml"
two_consumer_proof_required = true
"#,
    )
    .unwrap();
    let names = [
        "jeryu",
        "jeryu-cache",
        "jeryu-ci-runner",
        "jeryu-core",
        "jeryu-deploy",
        "jeryu-intelligence",
        "jeryu-jira",
        "jeryu-release-ops",
        "jeryu-tool",
        "jeryu-tool-finder",
        "jeryu-web",
    ];
    let repos = names
        .into_iter()
        .map(|name| {
            let mut repo = historical["repo"][0].clone();
            repo["name"] = Value::String(name.into());
            repo["path"] = Value::String(if name == "jeryu" {
                ".".into()
            } else {
                format!("components/{name}")
            });
            repo.as_table_mut()
                .unwrap()
                .insert("mirror_github_main".into(), Value::Boolean(false));
            repo
        })
        .collect();
    candidate
        .as_table_mut()
        .unwrap()
        .insert("repo".into(), Value::Array(repos));

    [("historical", historical), ("candidate", candidate)]
}

#[test]
fn manifest_command_requires_portal_membership_for_both_schemas() {
    use toml::Value;

    let temporary = tempfile::tempdir().unwrap();
    for (schema, valid) in portal_manifest_fixtures() {
        let path = temporary.path().join(format!("{schema}.toml"));
        let path_argument = path.to_str().unwrap();
        std::fs::write(&path, toml::to_string(&valid).unwrap()).unwrap();
        let output = run_manifest(&["--manifest", path_argument]);
        assert!(
            output.status.success(),
            "{schema}: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(!output.stdout.is_empty(), "{schema}: valid manifest output");

        let mut missing = valid.clone();
        missing.as_table_mut().unwrap().remove("required_repos");
        let mut invalid_manifests = vec![missing];
        for required in [
            Value::Array(Vec::new()),
            Value::Array(vec![Value::String("another-repo".into())]),
            Value::String("jeryu".into()),
            Value::Integer(1),
            Value::Array(vec![Value::Boolean(true)]),
        ] {
            let mut invalid = valid.clone();
            invalid["required_repos"] = required;
            invalid_manifests.push(invalid);
        }
        for invalid in invalid_manifests {
            std::fs::write(&path, toml::to_string(&invalid).unwrap()).unwrap();
            let output = run_manifest(&["--manifest", path_argument]);
            assert_eq!(output.status.code(), Some(1), "{schema}");
            assert!(output.stdout.is_empty(), "{schema}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("required_repos"),
                "{schema}: {}",
                String::from_utf8_lossy(&output.stderr),
            );
        }

        let mut empty = valid;
        empty["repo"] = Value::Array(Vec::new());
        std::fs::write(&path, toml::to_string(&empty).unwrap()).unwrap();
        let output = run_manifest(&["--manifest", path_argument]);
        assert_eq!(output.status.code(), Some(1), "{schema}: empty inventory");
        assert!(output.stdout.is_empty(), "{schema}: empty inventory");
    }
}

#[test]
fn monorepo_command_uses_current_directory_and_rejects_external_manifest_paths() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path();
    let manifest = &portal_manifest_fixtures()[1].1;
    let toolchain = "[toolchain]\nchannel='1.97.1'\n";
    std::fs::create_dir(root.join(".cargo")).unwrap();
    std::fs::write(root.join(".cargo/config.toml"), "[build]\njobs=2\n").unwrap();
    std::fs::write(root.join("rust-toolchain.toml"), toolchain).unwrap();
    std::fs::write(
        root.join("repos.manifest.toml"),
        toml::to_string(manifest).unwrap(),
    )
    .unwrap();
    for repo in manifest["repo"].as_array().unwrap() {
        if repo["name"].as_str() == Some("jeryu") {
            continue;
        }
        let component = root.join(repo["path"].as_str().unwrap());
        std::fs::create_dir_all(&component).unwrap();
        std::fs::write(component.join("rust-toolchain.toml"), toolchain).unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=[]\n[workspace.dependencies]\nexternal={path='../external'}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jeryu-split"))
        .current_dir(root)
        .arg("monorepo-check")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.contains("dependency path leaves this repository"),
        "{error}"
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("0 refreshed")
    );
    assert!(!root.join("Cargo.lock").exists());
}

#[test]
fn local_export_arguments_refuse_incomplete_or_duplicate_preparation() {
    let revision = "a".repeat(40);
    for extra in [
        vec!["--prepare-local"],
        vec!["--prepare-local", "/fixture/source"],
        vec![
            "--resolve-lock",
            "--prepare-local",
            "/fixture/source",
            "--prepare-local",
            "/fixture/other",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_jeryu-split"))
            .args([
                "export-tree",
                "--component",
                "jeryu-cache",
                "--source",
                &revision,
            ])
            .args(extra)
            .output()
            .expect("parse local export arguments");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("retained split lock scratch"));
    }
}
