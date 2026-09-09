use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const SOURCE: &str = "https://github.com/neverhuman/jeryu-core.git";
const HOSTED: &str = "https://git.neverhuman.org/git/jeryu/jeryu-core.git";
const TAG: &str = "jeryu-core-v5.0.0-split.0";
const COMMIT: &str = "0e29dc90673ffdf9959aaaa1f05482301f15fabe";
const SUPPORT_REF: &str = "refs/heads/preserve/hosted-cargo/jeryu-core-v5.0.0-split.0";
const GOVERNED_JANKURAI: &str = "/home/ubuntu/.jeryu/bin/jankurai";
const GOVERNED_JANKURAI_VERSION: &str = "jankurai 1.6.11";
const GOVERNED_JANKURAI_SHA256: &str =
    "9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c";

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "jeryu-ci-runner-hosted-transport-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create unique scratch directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

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
                target.clone().expect("insteadOf follows a URL section"),
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
            assert_eq!(fields.len(), 4, "hosted pin row has four fields");
            (
                fields[0].to_owned(),
                fields[1].to_owned(),
                fields[2].to_owned(),
                fields[3].to_owned(),
            )
        })
        .collect()
}

fn scrub_git_config_env(command: &mut Command) {
    for (name, _) in std::env::vars() {
        if name == "GIT_CONFIG"
            || name == "GIT_CONFIG_COUNT"
            || name == "GIT_CONFIG_PARAMETERS"
            || name == "GIT_CONFIG_SYSTEM"
            || name.starts_with("GIT_CONFIG_KEY_")
            || name.starts_with("GIT_CONFIG_VALUE_")
        {
            command.env_remove(name);
        }
    }
}

fn effective_url(overlay: &Path, source: &str) -> String {
    let mut command = Command::new("git");
    command
        .current_dir("/")
        .args(["ls-remote", "--get-url", source])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", overlay)
        .env("GIT_TERMINAL_PROMPT", "0");
    scrub_git_config_env(&mut command);
    let output = command.output().expect("run Git URL resolver");
    assert!(
        output.status.success(),
        "Git URL resolution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 URL")
        .trim()
        .to_owned()
}

fn source_helper(helper: &Path, global: &Path, body: &str) -> Output {
    let mut command = Command::new("bash");
    command
        .current_dir(root())
        .args(["-c", body, "_", helper.to_str().expect("UTF-8 helper")])
        .env("GIT_CONFIG_GLOBAL", global);
    scrub_git_config_env(&mut command);
    command.output().expect("run hosted Git environment helper")
}

fn write_fake_jankurai(path: &Path, version: &str, marker: &Path, mode: u32) {
    fs::create_dir_all(path.parent().expect("fake Jankurai parent"))
        .expect("create fake Jankurai bin directory");
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{version}'\nprintf ran > '{}'\n",
            marker.display()
        ),
    )
    .expect("write fake Jankurai");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("set fake Jankurai mode");
}

fn run_library_jankurai(
    earlier_path: &Path,
    governed_override: Option<&Path>,
    receipt_override: Option<&Path>,
    allow_test_receipt: bool,
) -> Output {
    let root = root();
    let mut command = Command::new("bash");
    command
        .args([
            "-lc",
            "export PATH=\"$2:$PATH\"; cd \"$1\"; source ops/ci/lib.sh; require_jankurai; printf 'command=%s\\nfile=%s\\nversion=' \"$(command -v jankurai)\" \"$(type -P -- jankurai)\"; jankurai --version",
            "_",
            root.to_str().expect("UTF-8 workspace root"),
            earlier_path.to_str().expect("UTF-8 hostile PATH"),
        ])
        .env("CARGO_HOME", earlier_path)
        .env("JERYU_JANKURAI_BIN", earlier_path.join("jankurai"))
        .env(
            "GIT_CONFIG_GLOBAL",
            root.join(".cargo/hosted-gitconfig"),
        )
        .env_remove("JAIN_RELEASE_CI")
        .env_remove("JERYU_GOVERNED_JANKURAI_BIN")
        .env_remove("JERYU_JANKURAI_RECEIPT")
        .env_remove("JERYU_JANKURAI_RECEIPT_SHA256")
        .env_remove("JERYU_JANKURAI_ALLOW_TEST_RECEIPT");
    if let Some(path) = governed_override {
        command.env("JERYU_GOVERNED_JANKURAI_BIN", path);
    }
    if let Some(path) = receipt_override {
        command.env("JERYU_JANKURAI_RECEIPT", path);
    }
    if allow_test_receipt {
        command.env("JERYU_JANKURAI_ALLOW_TEST_RECEIPT", "1");
    }
    scrub_git_config_env(&mut command);
    command.output().expect("run CI library in a login shell")
}

fn sha256_file(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("run sha256sum");
    assert!(output.status.success(), "sha256sum failed");
    String::from_utf8(output.stdout)
        .expect("UTF-8 sha256sum output")
        .split_whitespace()
        .next()
        .expect("SHA-256 field")
        .to_owned()
}

#[test]
fn cargo_identity_and_effective_transport_are_exact() {
    let root = root();
    let cargo_config = fs::read_to_string(root.join(".cargo/config.toml")).expect("Cargo config");
    assert!(cargo_config.contains("git-fetch-with-cli = true"));
    assert!(!cargo_config.contains("GIT_CONFIG_GLOBAL"));

    let overlay_path = root.join(".cargo/hosted-gitconfig");
    let overlay = fs::read_to_string(&overlay_path).expect("hosted Git overlay");
    let expected_mapping = BTreeSet::from([(SOURCE.to_owned(), HOSTED.to_owned())]);
    assert_eq!(parse_mappings(&overlay), expected_mapping);
    assert!(!overlay.contains("[include]"));
    assert!(!overlay.contains("[includeIf"));
    assert!(
        overlay.contains("helper = /home/ubuntu/.config/jeryu/bin/git-credential-neverhuman-org")
    );
    assert!(overlay.contains("[http \"https://git.neverhuman.org\"]\n\tpostBuffer = 1"));
    assert_eq!(effective_url(&overlay_path, SOURCE), HOSTED);

    let pin_policy =
        fs::read_to_string(root.join(".cargo/hosted-pin-refs.tsv")).expect("hosted pin policy");
    let expected_pin = BTreeSet::from([(
        "jeryu-core".to_owned(),
        TAG.to_owned(),
        COMMIT.to_owned(),
        SUPPORT_REF.to_owned(),
    )]);
    assert_eq!(parse_pins(&pin_policy), expected_pin);

    let workspace = root
        .ancestors()
        .find(|path| path.join("Cargo.lock").is_file())
        .expect("workspace lock");
    let lock = fs::read_to_string(workspace.join("Cargo.lock")).expect("Cargo lock");
    let expected_source = if workspace.join(".jeryu-source.json").is_file() {
        let provenance: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(workspace.join(".jeryu-source.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(provenance["component"], "jeryu-ci-runner");
        let source = provenance["source_commit"].as_str().unwrap();
        assert_eq!(source.len(), 40);
        assert!(source.bytes().all(|byte| byte.is_ascii_hexdigit()));
        Some(format!(
            "source = \"git+https://github.com/neverhuman/jeryu.git?rev={source}#{source}\""
        ))
    } else {
        None
    };
    // Runner consumes Core; Gitd is an additional witness in the full monorepo
    // and is correctly absent from Runner's standalone dependency closure.
    let witnesses: &[&str] = if expected_source.is_some() {
        &["jeryu-core"]
    } else {
        &["jeryu-core", "jeryu-gitd"]
    };
    for name in witnesses {
        let packages: Vec<_> = lock
            .split("[[package]]")
            .filter(|record| {
                record
                    .lines()
                    .any(|line| line == format!("name = \"{name}\""))
            })
            .collect();
        assert_eq!(packages.len(), 1, "duplicate workspace package identity");
        assert_eq!(
            packages[0]
                .lines()
                .find(|line| line.starts_with("source =")),
            expected_source.as_deref(),
            "monorepo packages must be local; split dependencies must bind the originating source commit"
        );
    }

    let deny = fs::read_to_string(root.join("deny.toml")).expect("Cargo Deny policy");
    assert!(deny.contains("unknown-git = \"deny\""));
    assert!(deny.contains(&format!("allow-git = [\"{SOURCE}\"]")));

    let missing = overlay.replacen(&format!("\tinsteadOf = {SOURCE}\n"), "", 1);
    assert_ne!(parse_mappings(&missing), expected_mapping);
    let wrong = overlay.replacen(HOSTED, "https://attacker.invalid/core.git", 1);
    assert_ne!(parse_mappings(&wrong), expected_mapping);
    let extra = format!(
        "{overlay}\n[url \"https://attacker.invalid/repo.git\"]\n\tinsteadOf = https://unlisted.invalid/repo.git\n"
    );
    assert_ne!(parse_mappings(&extra), expected_mapping);

    let missing_pin = pin_policy
        .lines()
        .filter(|line| line.trim().starts_with('#') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(parse_pins(&missing_pin), expected_pin);
    let wrong_pin = pin_policy.replacen(COMMIT, "0000000000000000000000000000000000000000", 1);
    assert_ne!(parse_pins(&wrong_pin), expected_pin);
    let extra_pin = format!(
        "{pin_policy}attacker|attacker-v1.0.0|1111111111111111111111111111111111111111|refs/heads/preserve/hosted-cargo/attacker-v1.0.0\n"
    );
    assert_ne!(parse_pins(&extra_pin), expected_pin);
}

#[test]
fn source_helper_rejects_unsafe_caller_configs_and_scrubs_injections() {
    let root = root();
    let helper = root.join("ops/ci/hosted-git-env.sh");
    let overlay = root.join(".cargo/hosted-gitconfig");
    let scratch = Scratch::new();
    let regular = scratch.0.join("regular.config");
    fs::copy(&overlay, &regular).expect("copy test config");
    let symlink_path = scratch.0.join("symlink.config");
    symlink(&regular, &symlink_path).expect("create test symlink");
    let hardlink_path = scratch.0.join("hardlink.config");
    fs::hard_link(&regular, &hardlink_path).expect("create test hardlink");
    let equivalent = scratch.0.join("equivalent.config");
    fs::copy(&overlay, &equivalent).expect("copy independent equivalent config");

    for invalid in [
        scratch.0.join("missing.config"),
        symlink_path,
        hardlink_path,
    ] {
        let output = source_helper(&helper, &invalid, "source \"$1\"");
        assert!(
            !output.status.success(),
            "unsafe caller config was accepted"
        );
    }
    let relative = source_helper(&helper, Path::new("relative.config"), "source \"$1\"");
    assert!(
        !relative.status.success(),
        "relative caller config was accepted"
    );

    let accepted = source_helper(
        &helper,
        &equivalent,
        "source \"$1\"; printf '%s' \"$GIT_CONFIG_GLOBAL\"",
    );
    assert!(
        accepted.status.success(),
        "equivalent caller config was rejected: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(
        String::from_utf8(accepted.stdout).expect("UTF-8 canonical config path"),
        overlay.to_str().expect("UTF-8 overlay path")
    );

    let mismatched = source_helper(&helper, &root.join("deny.toml"), "source \"$1\"");
    assert!(!mismatched.status.success());
    assert!(
        String::from_utf8_lossy(&mismatched.stderr).contains("differs from the exact reviewed")
    );

    let mut command = Command::new("bash");
    command
        .current_dir(&root)
        .args([
            "-c",
            "source \"$1\"; [[ -z ${GIT_CONFIG_COUNT+x} && -z ${GIT_CONFIG_KEY_0+x} && -z ${GIT_CONFIG_VALUE_0+x} && -z ${GIT_SSL_NO_VERIFY+x} && -z ${GIT_TRACE_CURL+x} && -z ${GIT_EXEC_PATH+x} && -z ${GIT_DIR+x} && -z ${GIT_OBJECT_DIRECTORY+x} && -z ${HTTPS_PROXY+x} && -z ${https_proxy+x} && -z ${CURL_CA_BUNDLE+x} && -z ${SSL_CERT_FILE+x} && $GIT_TERMINAL_PROMPT == 0 && $CARGO_NET_GIT_FETCH_WITH_CLI == true ]]",
            "_",
            helper.to_str().expect("UTF-8 helper"),
        ])
        .env("GIT_CONFIG_GLOBAL", &overlay)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "url.https://attacker.invalid.insteadOf")
        .env("GIT_CONFIG_VALUE_0", SOURCE)
        .env("GIT_SSL_NO_VERIFY", "1")
        .env("GIT_TRACE_CURL", "1")
        .env("GIT_EXEC_PATH", "/tmp/attacker-git-core")
        .env("GIT_DIR", "/tmp/attacker.git")
        .env("GIT_OBJECT_DIRECTORY", "/tmp/attacker-objects")
        .env("HTTPS_PROXY", "http://attacker.invalid:8080")
        .env("https_proxy", "http://attacker.invalid:8081")
        .env("CURL_CA_BUNDLE", "/tmp/attacker-ca.pem")
        .env("SSL_CERT_FILE", "/tmp/attacker-ca.pem")
        .env("GIT_TERMINAL_PROMPT", "1")
        .env("CARGO_NET_GIT_FETCH_WITH_CLI", "false");
    let scrubbed = command.output().expect("run injected helper test");
    assert!(
        scrubbed.status.success(),
        "Git config injection was not scrubbed: {}",
        String::from_utf8_lossy(&scrubbed.stderr)
    );
}

#[test]
fn login_shell_proof_replay_uses_only_the_governed_jankurai_binary() {
    let scratch = Scratch::new();
    let hostile_bin = scratch.0.join("hostile-bin");
    let hostile = hostile_bin.join("jankurai");
    let hostile_marker = scratch.0.join("hostile-ran");
    write_fake_jankurai(&hostile, "jankurai 99.0.0", &hostile_marker, 0o755);

    let output = run_library_jankurai(&hostile_bin, None, None, false);
    assert!(
        output.status.success(),
        "governed Jankurai was not selected: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("UTF-8 Jankurai version"),
        format!(
            "command=jankurai\nfile={GOVERNED_JANKURAI}\nversion={GOVERNED_JANKURAI_VERSION}\n"
        )
    );
    assert_eq!(
        sha256_file(Path::new(GOVERNED_JANKURAI)),
        GOVERNED_JANKURAI_SHA256
    );
    assert!(
        !hostile_marker.exists(),
        "an earlier PATH Jankurai was executed"
    );
}

#[test]
fn governed_jankurai_custody_identity_and_receipt_fail_closed() {
    let scratch = Scratch::new();
    let hostile_bin = scratch.0.join("hostile-bin");
    let hostile_marker = scratch.0.join("hostile-ran");
    write_fake_jankurai(
        &hostile_bin.join("jankurai"),
        "jankurai 99.0.0",
        &hostile_marker,
        0o755,
    );

    let missing = scratch.0.join("missing-jankurai");
    let missing_result = run_library_jankurai(&hostile_bin, Some(&missing), None, false);
    assert!(
        !missing_result.status.success(),
        "missing governed binary was accepted"
    );

    let symlink_target = scratch.0.join("symlink-target");
    write_fake_jankurai(
        &symlink_target,
        GOVERNED_JANKURAI_VERSION,
        &scratch.0.join("symlink-ran"),
        0o755,
    );
    let symlink_path = scratch.0.join("symlink-jankurai");
    symlink(&symlink_target, &symlink_path).expect("create Jankurai symlink");
    assert!(
        !run_library_jankurai(&hostile_bin, Some(&symlink_path), None, false)
            .status
            .success(),
        "symlinked governed binary was accepted"
    );

    let symlink_parent_target = scratch.0.join("symlink-parent-target");
    write_fake_jankurai(
        &symlink_parent_target.join("bin/jankurai"),
        GOVERNED_JANKURAI_VERSION,
        &scratch.0.join("symlink-parent-ran"),
        0o755,
    );
    let symlink_parent_home = scratch.0.join("symlink-parent-home");
    symlink(&symlink_parent_target, &symlink_parent_home).expect("create Jankurai parent symlink");
    assert!(
        !run_library_jankurai(
            &hostile_bin,
            Some(&symlink_parent_home.join("bin/jankurai")),
            None,
            false,
        )
        .status
        .success(),
        "governed binary beneath a symlinked parent was accepted"
    );

    let hardlink_path = scratch.0.join("hardlink/jankurai");
    fs::create_dir_all(hardlink_path.parent().expect("hardlink parent"))
        .expect("create hardlink directory");
    fs::copy(GOVERNED_JANKURAI, &hardlink_path).expect("copy governed Jankurai fixture");
    let hardlink_alias = scratch.0.join("hardlink/jankurai-alias");
    fs::hard_link(&hardlink_path, &hardlink_alias).expect("create Jankurai hardlink");
    let hardlink_result = run_library_jankurai(&hostile_bin, Some(&hardlink_path), None, false);
    assert!(
        !hardlink_result.status.success(),
        "hard-linked governed binary was accepted"
    );
    assert!(
        String::from_utf8_lossy(&hardlink_result.stderr)
            .contains("governed jankurai custody mismatch: expected one link"),
        "hard-linked governed binary did not fail at custody validation: {}",
        String::from_utf8_lossy(&hardlink_result.stderr)
    );

    let non_executable = scratch.0.join("non-executable-jankurai");
    write_fake_jankurai(
        &non_executable,
        GOVERNED_JANKURAI_VERSION,
        &scratch.0.join("non-executable-ran"),
        0o644,
    );
    assert!(
        !run_library_jankurai(&hostile_bin, Some(&non_executable), None, false)
            .status
            .success(),
        "non-executable governed binary was accepted"
    );

    let wrong_version_path = scratch.0.join("wrong-version/jankurai");
    write_fake_jankurai(
        &wrong_version_path,
        "jankurai 1.6.10",
        &scratch.0.join("wrong-version-ran"),
        0o755,
    );
    let wrong_version = run_library_jankurai(&hostile_bin, Some(&wrong_version_path), None, false);
    assert!(
        !wrong_version.status.success(),
        "wrong governed version was accepted"
    );
    let wrong_version_stderr = String::from_utf8_lossy(&wrong_version.stderr);
    assert!(
        wrong_version_stderr.contains("governed jankurai identity mismatch"),
        "unexpected wrong-version failure: {wrong_version_stderr}"
    );

    let wrong_digest_path = scratch.0.join("wrong-digest/jankurai");
    write_fake_jankurai(
        &wrong_digest_path,
        GOVERNED_JANKURAI_VERSION,
        &scratch.0.join("wrong-digest-ran"),
        0o755,
    );
    assert!(
        !run_library_jankurai(&hostile_bin, Some(&wrong_digest_path), None, false)
            .status
            .success(),
        "wrong governed digest was accepted"
    );

    assert!(
        !run_library_jankurai(&hostile_bin, None, Some(Path::new("/dev/null")), false)
            .status
            .success(),
        "malformed explicit receipt was accepted"
    );
    assert!(
        !run_library_jankurai(&hostile_bin, None, None, true)
            .status
            .success(),
        "release receipt was accepted as test-mode authority"
    );
    assert!(
        !hostile_marker.exists(),
        "hostile PATH binary ran during a fail-closed case"
    );
}
