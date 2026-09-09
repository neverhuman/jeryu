use super::auth::{canonical_askpass_prompt, read_token_file_with_hook};
use super::*;
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
pub(super) fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = env::temp_dir().join(format!(
        "jeryu-tool-render-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("create test root");
    root
}

#[cfg(unix)]
fn write_private(path: &Path, value: &str) {
    fs::write(path, value).expect("write fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).expect("chmod fixture");
}

#[cfg(unix)]
pub(super) fn run_fixture_git(root: &Path, args: &[&str]) {
    let mut command = local_git_command(root);
    let status = command.args(args).status().expect("run fixture Git");
    assert!(status.success(), "fixture Git failed: {args:?}");
}

#[test]
fn template_and_future_identity_are_shape_driven() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let pin = Pin::load(&root).expect("pin");
    let function = require_function(&root).expect("function");
    let script = ensure_script(&pin, &function);
    assert!(!pin.shell_block().contains("JERYU_GOVERNED_JANKURAI_BIN"));
    assert!(function.contains("/opt/jain-ci/authority/release-bin/jankurai"));
    assert!(function.contains("/home/ubuntu/.jeryu/bin/jankurai"));
    assert!(function.contains("local mode=receipt-bound"));
    assert!(script.contains(&function));
    let future = "Jankurai 9.9.9 / jankurai 9.9.9 / \
v9.9.9-deadlang-precision-split.9 / https://github.com/neverhuman/jankurai.git\n";
    let rendered = crate::render_rules::semantic_identity_rules(future, &pin);
    assert!(!rendered.contains("9.9.9"));
    assert!(rendered.contains(pin.get("version")));
    assert!(rendered.contains(pin.get("tag")));
    assert!(rendered.contains(pin.get("repo")));
    assert_eq!(
        sha256_bytes(b"abc").expect("SHA-256"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn hosted_repository_origin_is_exact_and_fail_closed() {
    let expected = "https://git.neverhuman.org/git/jeryu/jeryu-tool.git";
    assert_eq!(
        canonical_hosted_origin("jeryu-tool").expect("canonical hosted origin"),
        expected
    );
    assert_eq!(
        require_canonical_hosted_origin("jeryu-tool", expected).expect("exact hosted origin"),
        expected
    );

    for hostile in [
        "http://127.0.0.1:8787/git/jeryu/jeryu-tool.git",
        "http://git.neverhuman.org/git/jeryu/jeryu-tool.git",
        "https://jepsont@git.neverhuman.org/git/jeryu/jeryu-tool.git",
        "https://git.neverhuman.org.evil.invalid/git/jeryu/jeryu-tool.git",
        "https://git.neverhuman.org/git/veox/jeryu-tool.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-web.git",
        "https://git.neverhuman.org/git/jeryu/jeryu-tool",
        "https://git.neverhuman.org/git/jeryu/jeryu-tool.git/",
        "https://git.neverhuman.org/git/jeryu/jeryu-tool.git?ref=main",
        "https://git.neverhuman.org/git/jeryu/jeryu-tool.git#main",
        "ssh://git@git.neverhuman.org/jeryu/jeryu-tool.git",
    ] {
        assert!(
            require_canonical_hosted_origin("jeryu-tool", hostile).is_err(),
            "hostile origin was accepted: {hostile}"
        );
    }
    assert!(canonical_hosted_origin("../jeryu-tool").is_err());
    assert!(canonical_hosted_origin("jeryu-tool.git").is_err());
    assert!(canonical_hosted_origin("Jeryu-Tool").is_err());

    assert!(canonical_askpass_prompt(
        "Password for 'https://git@git.neverhuman.org/git/jeryu/jeryu-tool.git': "
    ));
    for hostile in [
        "Username for 'https://git.neverhuman.org': ",
        "Password for 'https://git.neverhuman.org': ",
        "Password for 'https://git@git.neverhuman.org': ",
        "Password for 'https://git@git.neverhuman.org/git/veox/jeryu-tool.git': ",
        "Password for 'https://git@git.neverhuman.org/git/jeryu/../jeryu-tool.git': ",
        "Password for 'https://git@git.neverhuman.org/git/jeryu/jeryu-tool': ",
        "Password for 'https://git@git.neverhuman.org/git/jeryu/jeryu-tool.git/': ",
    ] {
        assert!(
            !canonical_askpass_prompt(hostile),
            "hostile credential prompt was accepted: {hostile}"
        );
    }
}

#[test]
fn hosted_git_child_rejects_ambient_transport_and_process_injection() {
    let mut command = Command::new("/usr/bin/env");
    for key in SCRUBBED_GIT_ENVIRONMENT {
        command.env(key, "attacker-controlled");
    }
    scrub_git_environment(&mut command);

    let environment: BTreeMap<_, _> = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|item| item.to_string_lossy().into_owned()),
            )
        })
        .collect();
    for key in SCRUBBED_GIT_ENVIRONMENT {
        if matches!(*key, "NO_PROXY" | "no_proxy") {
            assert_eq!(
                environment.get(*key),
                Some(&Some(DIRECT_HOST_NO_PROXY.to_owned())),
                "direct-host no-proxy policy drifted for {key}"
            );
        } else {
            assert_eq!(
                environment.get(*key),
                Some(&None),
                "hostile child environment survived: {key}"
            );
        }
    }

    let command = hosted_git_command(
        "https://git@git.neverhuman.org/git/jeryu/jeryu-tool.git",
        Path::new("/proc/self/exe"),
    );
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.windows(2)
            .any(|args| args == ["-c", "http.sslVerify=true"])
    );
    assert!(args.windows(2).any(|args| args == ["-c", "http.proxy="]));
    assert!(
        args.windows(2)
            .any(|args| args == ["-c", "http.followRedirects=false"])
    );
}

#[cfg(unix)]
#[test]
fn protected_main_commit_accepts_harness_contract_base_without_origin() {
    let root = test_root("contract-base");
    run_fixture_git(&root, &["init", "-q"]);
    run_fixture_git(&root, &["config", "user.name", "Jeryu Test"]);
    run_fixture_git(&root, &["config", "user.email", "jeryu-test@invalid"]);
    fs::write(root.join("marker.txt"), "fixture\n").expect("write fixture");
    run_fixture_git(&root, &["add", "marker.txt"]);
    run_fixture_git(&root, &["commit", "-q", "-m", "fixture"]);
    let head = git_local_output(&root, &["rev-parse", "HEAD"]).expect("head");
    let resolved = protected_main_commit(&root, "jeryu-tool", false, Some(&head), false)
        .expect("harness contract base");
    assert_eq!(resolved, head);
    fs::remove_dir_all(root).expect("remove test root");
}

#[cfg(unix)]
#[test]
fn protected_main_commit_rejects_release_without_contract_base_or_origin() {
    let root = test_root("release-no-base");
    run_fixture_git(&root, &["init", "-q"]);
    let error = protected_main_commit(&root, "jeryu-tool", false, None, true)
        .expect_err("release without origin must fail closed");
    assert!(error.contains("JAIN_CONTRACT_BASE_REF"));
    fs::remove_dir_all(root).expect("remove test root");
}

#[cfg(unix)]
#[test]
fn credential_file_custody_rejects_mode_links_and_path_swaps() {
    let held = held_askpass_executable().expect("held askpass executable");
    assert_eq!(
        held,
        PathBuf::from(format!("/proc/{}/exe", std::process::id()))
    );
    let root = test_root("credential");
    let token = root.join("token");
    write_private(&token, "fixture-token\n");
    assert_eq!(
        read_token_file_with_hook(&token, || {}).expect("private token"),
        "fixture-token"
    );

    fs::set_permissions(&token, fs::Permissions::from_mode(0o640)).expect("chmod hostile");
    assert!(read_token_file_with_hook(&token, || {}).is_err());
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).expect("restore mode");

    let hardlink = root.join("hardlink");
    fs::hard_link(&token, &hardlink).expect("hardlink fixture");
    assert!(read_token_file_with_hook(&token, || {}).is_err());
    fs::remove_file(&hardlink).expect("remove hardlink");

    let link = root.join("symlink");
    symlink(&token, &link).expect("symlink fixture");
    assert!(read_token_file_with_hook(&link, || {}).is_err());

    let old = root.join("old-token");
    let replacement = token.clone();
    let result = read_token_file_with_hook(&token, || {
        fs::rename(&replacement, &old).expect("move opened token");
        write_private(&replacement, "replacement-token\n");
    });
    assert!(result.is_err());
    fs::remove_dir_all(root).expect("remove test root");
}

#[cfg(unix)]
#[test]
fn local_git_checks_disable_checkout_execution_and_reject_git_files() {
    let root = test_root("local-git");
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(&root)
        .status()
        .expect("git init");
    assert!(status.success());
    let marker = root.join("fsmonitor-ran");
    let monitor = root.join("monitor.sh");
    fs::write(
        &monitor,
        format!("#!/bin/sh\ntouch '{}'\nprintf '0\\n'\n", marker.display()),
    )
    .expect("write monitor");
    fs::set_permissions(&monitor, fs::Permissions::from_mode(0o700)).expect("chmod monitor");
    let status = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["config", "core.fsmonitor", monitor.to_str().expect("UTF-8")])
        .status()
        .expect("configure fsmonitor");
    assert!(status.success());
    git_local_output(&root, &["status", "--porcelain"]).expect("scrubbed status");
    assert!(!marker.exists(), "checkout-local fsmonitor executed");

    fs::remove_dir_all(root.join(".git")).expect("remove fixture metadata");
    fs::write(root.join(".git"), "gitdir: /tmp/attacker\n").expect("write gitfile");
    assert!(validate_repository_storage(&root).is_err());
    fs::remove_dir_all(root).expect("remove test root");
}

#[cfg(unix)]
#[test]
fn rendered_changes_reject_hidden_index_state_without_mutating_bytes() {
    let root = test_root("hidden-index");
    let status = Command::new(GIT_BIN)
        .args(["init", "-q"])
        .arg(&root)
        .status()
        .expect("git init");
    assert!(status.success());
    run_fixture_git(&root, &["config", "user.name", "Jeryu Test"]);
    run_fixture_git(&root, &["config", "user.email", "jeryu-test@invalid"]);
    let target = root.join("generated.txt");
    fs::write(&target, "original\n").expect("write tracked target");
    run_fixture_git(&root, &["add", "generated.txt"]);
    run_fixture_git(&root, &["commit", "-q", "-m", "fixture"]);

    let targets = vec![(root.clone(), vec![target.clone()])];
    validate_write_targets(&targets).expect("ordinary tracked target");

    for (set_flag, clear_flag) in [
        ("--assume-unchanged", "--no-assume-unchanged"),
        ("--skip-worktree", "--no-skip-worktree"),
    ] {
        run_fixture_git(&root, &["update-index", set_flag, "generated.txt"]);
        let hostile = format!("hidden by {set_flag}\n");
        fs::write(&target, &hostile).expect("write hostile hidden bytes");
        assert!(
            git_local_output(&root, &["status", "--porcelain"])
                .expect("hidden status")
                .is_empty(),
            "fixture change was not hidden by {set_flag}"
        );
        assert!(validate_index_state("fixture", &root).is_err());
        assert!(validate_write_target(&root, &target).is_err());
        let replacement = vec![(target.clone(), "rendered\n".to_owned())];
        assert!(apply_rendered_changes(&targets, &replacement).is_err());
        assert_eq!(
            fs::read_to_string(&target).expect("read target after rejection"),
            hostile
        );
        run_fixture_git(&root, &["update-index", clear_flag, "generated.txt"]);
        fs::write(&target, "original\n").expect("restore target");
        validate_write_targets(&targets).expect("restored ordinary target");
    }

    fs::remove_dir_all(root).expect("remove test root");
}
