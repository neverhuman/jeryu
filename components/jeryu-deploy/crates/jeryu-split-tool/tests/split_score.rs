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
fn score_exports_bind_every_shared_helper_to_the_selected_git_source() {
    let temporary = tempfile::Builder::new()
        .prefix("jeryu-split-score-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = temporary.path();
    let helpers = [
        ("scripts/check-audit-score.sh", "committed score adapter\n"),
        ("scripts/source-build.sh", "committed source digest\n"),
        ("tests/scratch.sh", "committed scratch custody\n"),
    ];
    for (path, content) in [
        ("Cargo.toml", "[workspace]\nmembers = [\"components/jeryu-jira/crates/example\"]\n[workspace.dependencies]\nexample = { path = \"components/jeryu-jira/crates/example\" }\n"),
        ("Cargo.lock", "version = 4\n"),
        ("LICENSE", "Apache-2.0 fixture\n"),
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.97.1\"\n"),
        (".cargo/config.toml", "[build]\njobs = 2\n"),
        ("components/jeryu-jira/AGENTS.md", "Work fixture\n"),
        ("components/jeryu-jira/README.md", "Work fixture\n"),
        ("components/jeryu-jira/crates/example/Cargo.toml", "[package]\nname = \"example\"\nversion = \"0.0.0\"\n"),
    ].into_iter().chain(helpers) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
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
            "Synthetic score source",
        ],
    );
    let source = git(root, &["rev-parse", "HEAD"]);
    let source = source.trim();
    let component_tree = git(root, &["rev-parse", "HEAD:components/jeryu-jira"]);
    for (path, _) in helpers {
        fs::write(root.join(path), "uncommitted replacement\n").unwrap();
    }
    let mut trees = Vec::new();
    for _ in 0..2 {
        let output = fixture_command(
            env!("CARGO_BIN_EXE_jeryu-split"),
            root,
            &[
                "export-tree",
                "--component",
                "jeryu-jira",
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
    for (path, expected) in helpers {
        assert_eq!(git(root, &["show", &format!("{tree}:{path}")]), expected);
        assert_eq!(
            fs::read_to_string(root.join(path)).unwrap(),
            "uncommitted replacement\n"
        );
    }
    let descriptor: serde_json::Value =
        serde_json::from_str(&git(root, &["show", &format!("{tree}:.jeryu-source.json")])).unwrap();
    assert_eq!(descriptor["source_commit"], source);
    assert_eq!(descriptor["original_component_tree"], component_tree.trim());
    let readme = git(root, &["show", &format!("{tree}:README.md")]);
    assert!(readme.contains("--prepare-local /absolute/clean/monorepo"));
    assert!(readme.contains("not anonymous qualification"));
    assert_eq!(git(root, &["rev-parse", "HEAD"]).trim(), source);
}
