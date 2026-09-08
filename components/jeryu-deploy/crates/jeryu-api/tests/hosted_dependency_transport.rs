use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_LOCK_SOURCES: [&str; 6] = [
    "http://127.0.0.1:8787/git/jeryu/jeryu-core.git",
    "http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git",
    "https://github.com/neverhuman/jeryu-ci-runner.git",
    "https://github.com/neverhuman/jeryu-intelligence.git",
    "https://github.com/neverhuman/jeryu-jira.git",
    "https://github.com/neverhuman/jeryu-release-ops.git",
];

const EXPECTED_MAPPINGS: [(&str, &str); 7] = [
    (
        "http://127.0.0.1:8787/git/jeryu/jeryu-core.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-core.git",
    ),
    (
        "https://github.com/neverhuman/jeryu-core.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-core.git",
    ),
    (
        "http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-intelligence.git",
    ),
    (
        "https://github.com/neverhuman/jeryu-intelligence.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-intelligence.git",
    ),
    (
        "https://github.com/neverhuman/jeryu-ci-runner.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-ci-runner.git",
    ),
    (
        "https://github.com/neverhuman/jeryu-jira.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-jira.git",
    ),
    (
        "https://github.com/neverhuman/jeryu-release-ops.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-release-ops.git",
    ),
];

const EXPECTED_PINS: [(&str, &str, &str, &str); 5] = [
    (
        "jeryu-ci-runner",
        "jeryu-ci-runner-v5.0.0-split.0",
        "8bd66f1d2d71621996de8af260611f36da849fb8",
        "refs/heads/preserve/hosted-cargo/jeryu-ci-runner-v5.0.0-split.0",
    ),
    (
        "jeryu-core",
        "jeryu-core-v5.0.0-split.5",
        "4582e10ff92ddd8b8e5c2dfdba090eea53f55cbc",
        "refs/heads/preserve/hosted-cargo/jeryu-core-v5.0.0-split.5",
    ),
    (
        "jeryu-intelligence",
        "jeryu-intelligence-v5.0.0-split.1",
        "6fb845c594c3e5e9ffea8047d8a3f814fa9ba4da",
        "refs/heads/preserve/hosted-cargo/jeryu-intelligence-v5.0.0-split.1",
    ),
    (
        "jeryu-jira",
        "jeryu-jira-v5.0.0-split.0",
        "669d3a2261ae6570c518c95169feaebd4ab23956",
        "refs/heads/preserve/hosted-cargo/jeryu-jira-v5.0.0-split.0",
    ),
    (
        "jeryu-release-ops",
        "jeryu-release-ops-v5.0.0-split.0",
        "6f57a3153a9c64c7191f4fbc6c7a4e44767a8cac",
        "refs/heads/preserve/hosted-cargo/jeryu-release-ops-v5.0.0-split.0",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn parse_mappings(contents: &str) -> BTreeSet<(String, String)> {
    let mut target = None;
    let mut mappings = BTreeSet::new();
    for line in contents.lines().map(str::trim) {
        if let Some(value) = line
            .strip_prefix("[url \"")
            .and_then(|value| value.strip_suffix("\"]"))
        {
            target = Some(value.to_owned());
        } else if let Some(source) = line.strip_prefix("insteadOf = ") {
            mappings.insert((
                source.to_owned(),
                target.clone().expect("insteadOf must follow a url section"),
            ));
        }
    }
    mappings
}

fn parse_pins(contents: &str) -> BTreeSet<(String, String, String, String)> {
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields = line.split('|').collect::<Vec<_>>();
            assert_eq!(fields.len(), 4, "hosted pin row must have four fields");
            (
                fields[0].to_owned(),
                fields[1].to_owned(),
                fields[2].to_owned(),
                fields[3].to_owned(),
            )
        })
        .collect()
}

fn effective_url(overlay: &Path, source: &str, cwd: &Path) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .args(["ls-remote", "--get-url", source])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_GLOBAL", overlay)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS");
    for (name, _) in std::env::vars() {
        if name.starts_with("GIT_CONFIG_KEY_") || name.starts_with("GIT_CONFIG_VALUE_") {
            command.env_remove(name);
        }
    }
    let output = command.output().expect("run git URL resolver");
    assert!(
        output.status.success(),
        "git URL resolution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 URL")
        .trim()
        .to_owned()
}

#[test]
fn cargo_sources_are_exact_immutable_and_hosted_in_transport() {
    let root = root();
    let cargo_config: toml::Value = toml::from_str(
        &fs::read_to_string(root.join(".cargo/config.toml")).expect("read Cargo config"),
    )
    .expect("parse Cargo config");
    assert_eq!(
        cargo_config["net"]["git-fetch-with-cli"].as_bool(),
        Some(true)
    );
    assert!(
        cargo_config
            .get("env")
            .and_then(|env| env.get("GIT_CONFIG_GLOBAL"))
            .is_none(),
        "Cargo's env table cannot govern Cargo's own Git fetch"
    );

    let helper = fs::read_to_string(root.join("ops/ci/hosted-git-env.sh"))
        .expect("read hosted Git environment helper");
    assert!(helper.contains("export GIT_CONFIG_GLOBAL=\"${hosted_git_env_overlay}\""));
    assert!(helper.contains("if [[ -v GIT_CONFIG_GLOBAL ]]"));
    assert!(helper.contains("unset GIT_CONFIG GIT_CONFIG_COUNT GIT_CONFIG_PARAMETERS"));
    assert!(helper.contains("export GIT_CONFIG_NOSYSTEM=1"));
    let gate = fs::read_to_string(root.join("ops/ci/dependency-sources.sh"))
        .expect("read dependency source gate");
    assert!(gate.contains("active_global=\"${GIT_CONFIG_GLOBAL}\""));
    assert!(gate.contains("GIT_CONFIG_GLOBAL=\"${active_global}\""));
    assert!(gate.contains("active_git_config_sha256"));

    let overlay_path = root.join(".cargo/hosted-gitconfig");
    let overlay = fs::read_to_string(&overlay_path).expect("read hosted Git overlay");
    assert!(!overlay.contains("[include]"));
    assert!(
        overlay.contains("helper = /home/ubuntu/.config/jeryu/bin/git-credential-neverhuman-org")
    );
    assert!(overlay.contains("[http \"https://git.neverhuman.org\"]\n\tpostBuffer = 1"));
    let expected_mappings = EXPECTED_MAPPINGS
        .into_iter()
        .map(|(source, target)| (source.to_owned(), target.to_owned()))
        .collect::<BTreeSet<_>>();
    assert_eq!(parse_mappings(&overlay), expected_mappings);

    let pin_policy = fs::read_to_string(root.join(".cargo/hosted-pin-refs.tsv"))
        .expect("read hosted pin policy");
    let expected_pins = EXPECTED_PINS
        .into_iter()
        .map(|(repo, tag, commit, reference)| {
            (
                repo.to_owned(),
                tag.to_owned(),
                commit.to_owned(),
                reference.to_owned(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(parse_pins(&pin_policy), expected_pins);
    for (repo, tag, commit, reference) in &expected_pins {
        assert!(repo.starts_with("jeryu-"));
        assert!(!tag.is_empty());
        assert_eq!(commit.len(), 40);
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(
            reference,
            &format!("refs/heads/preserve/hosted-cargo/{tag}")
        );
    }

    let scratch = tempfile::tempdir().expect("temporary non-repository directory");
    for (source, expected) in EXPECTED_MAPPINGS {
        assert_eq!(
            effective_url(&overlay_path, source, scratch.path()),
            expected
        );
    }

    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.lock")).expect("read Cargo.lock"))
            .expect("parse Cargo.lock");
    let mut lock_sources = BTreeSet::new();
    let mut lock_pins = BTreeSet::new();
    for package in lock["package"].as_array().expect("lock packages") {
        let Some(source) = package.get("source").and_then(toml::Value::as_str) else {
            continue;
        };
        let Some(git_source) = source.strip_prefix("git+") else {
            continue;
        };
        let (base, identity) = git_source.split_once('?').expect("tagged Git source");
        let (tag, commit) = identity
            .strip_prefix("tag=")
            .and_then(|value| value.split_once('#'))
            .expect("tag and commit identity");
        assert!(!tag.is_empty());
        assert_eq!(commit.len(), 40);
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        lock_sources.insert(base.to_owned());
        lock_pins.insert((tag.to_owned(), commit.to_owned()));
    }
    let expected_sources = EXPECTED_LOCK_SOURCES
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    assert_eq!(lock_sources, expected_sources);
    let expected_lock_pins = expected_pins
        .iter()
        .map(|(_, tag, commit, _)| (tag.clone(), commit.clone()))
        .collect::<BTreeSet<_>>();
    assert_eq!(lock_pins, expected_lock_pins);

    let deny: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("deny.toml")).expect("read deny policy"))
            .expect("parse deny policy");
    assert_eq!(deny["sources"]["unknown-git"].as_str(), Some("deny"));
    let allowed = deny["sources"]["allow-git"]
        .as_array()
        .expect("Git allowlist")
        .iter()
        .map(|value| value.as_str().expect("string Git source").to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(allowed, expected_sources);

    let missing = overlay.replacen(
        "\tinsteadOf = https://github.com/neverhuman/jeryu-ci-runner.git\n",
        "",
        1,
    );
    assert_ne!(parse_mappings(&missing), expected_mappings);
    let extra = format!(
        "{overlay}\n[url \"https://attacker.invalid/repo.git\"]\n\tinsteadOf = https://unlisted.invalid/repo.git\n"
    );
    assert_ne!(parse_mappings(&extra), expected_mappings);

    let missing_pin = pin_policy.replacen(
        "jeryu-ci-runner|jeryu-ci-runner-v5.0.0-split.0|8bd66f1d2d71621996de8af260611f36da849fb8|refs/heads/preserve/hosted-cargo/jeryu-ci-runner-v5.0.0-split.0\n",
        "",
        1,
    );
    assert_ne!(parse_pins(&missing_pin), expected_pins);
    let wrong_pin = pin_policy.replacen(
        "8bd66f1d2d71621996de8af260611f36da849fb8",
        "0000000000000000000000000000000000000000",
        1,
    );
    assert_ne!(parse_pins(&wrong_pin), expected_pins);
    let extra_pin = format!(
        "{pin_policy}attacker|attacker-v1.0.0|1111111111111111111111111111111111111111|refs/heads/preserve/hosted-cargo/attacker-v1.0.0\n"
    );
    assert_ne!(parse_pins(&extra_pin), expected_pins);
}
