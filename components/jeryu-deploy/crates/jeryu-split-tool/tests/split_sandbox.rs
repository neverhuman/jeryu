use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

fn fixture_command(program: &str, root: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").expect("test PATH"))
        .env("HOME", root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .output()
        .expect("run local fixture command")
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = fixture_command("git", root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn runner_export_preserves_the_committed_shared_sandbox_entrypoint() {
    let temporary = tempfile::Builder::new()
        .prefix("jeryu-split-sandbox-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = temporary.path();
    let helper = "components/jeryu-ci-runner/scripts/test-native-sandbox.sh";
    let committed = "#!/usr/bin/env bash\nprintf 'committed Runner proof\\n'\n";
    for (path, content) in [
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"components/jeryu-ci-runner/crates/example\"]\n[workspace.dependencies]\nexample = { path = \"components/jeryu-ci-runner/crates/example\" }\n",
        ),
        ("Cargo.lock", "version = 4\n"),
        ("LICENSE", "Apache-2.0 fixture\n"),
        ("scripts/source-build.sh", "committed source helper\n"),
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.97.1\"\n"),
        (".cargo/config.toml", "[build]\njobs = 2\n"),
        ("components/jeryu-ci-runner/AGENTS.md", "Runner fixture\n"),
        ("components/jeryu-ci-runner/README.md", "Runner fixture\n"),
        (
            "components/jeryu-ci-runner/crates/example/Cargo.toml",
            "[package]\nname = \"example\"\nversion = \"0.0.0\"\n",
        ),
        (helper, committed),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fs::set_permissions(root.join(helper), fs::Permissions::from_mode(0o755)).unwrap();
    git(
        root,
        &["init", "--quiet", "--initial-branch=main", "--template="],
    );
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Split fixture",
            "-c",
            "user.email=split-fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "Synthetic Runner source",
        ],
    );
    let source = git(root, &["rev-parse", "HEAD"]);
    let source = source.trim();
    let uncommitted = "#!/usr/bin/env bash\nprintf 'uncommitted Runner proof\\n'\n";
    fs::write(root.join(helper), uncommitted).unwrap();

    let mut trees = Vec::new();
    for _ in 0..2 {
        let output = fixture_command(
            env!("CARGO_BIN_EXE_jeryu-split"),
            root,
            &[
                "export-tree",
                "--component",
                "jeryu-ci-runner",
                "--source",
                source,
            ],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["source_commit"], source);
        assert_eq!(result["lock_regeneration_required"], true);
        assert_eq!(result["publication_qualified"], false);
        trees.push(result["tree"].as_str().unwrap().to_owned());
    }
    assert_eq!(trees[0], trees[1]);
    let tree = &trees[0];
    let exported_helper = "scripts/test-native-sandbox.sh";
    assert_eq!(
        git(root, &["show", &format!("{tree}:{exported_helper}")]),
        committed
    );
    assert!(git(root, &["ls-tree", tree, "--", exported_helper]).starts_with("100755 blob "));
    let ci = git(root, &["show", &format!("{tree}:scripts/split-ci.sh")]);
    assert_eq!(ci, include_str!("../src/split_ci.sh"));
    let sandbox = ci
        .split("  sandbox)\n")
        .nth(1)
        .unwrap()
        .split("    ;;\n")
        .next()
        .unwrap();
    assert!(sandbox.contains("bash scripts/test-native-sandbox.sh --workspace-root \"$(pwd -P)\""));
    assert!(!sandbox.contains("cargo "));
    assert!(!sandbox.contains("enforcement.json"));
    assert_eq!(git(root, &["rev-parse", "HEAD"]).trim(), source);
    assert_eq!(fs::read_to_string(root.join(helper)).unwrap(), uncommitted);
}
