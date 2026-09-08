//! Public command-line contract: the checked-in help text must change in the
//! same review as any user-visible command surface.

use std::process::Command;

fn release_tag() -> &'static str {
    include_str!("../VERSION")
        .strip_suffix('\n')
        .expect("VERSION has exactly one trailing newline")
}

#[test]
fn top_level_help_matches_tracked_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_jeryu-tool-finder"))
        .arg("--help")
        .output()
        .expect("run jeryu-tool-finder --help");

    assert!(
        output.status.success(),
        "--help failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "--help wrote to stderr");
    assert_eq!(
        String::from_utf8(output.stdout).expect("help is UTF-8"),
        include_str!("../contracts/cli-help.txt"),
        "CLI help drifted; review and update contracts/cli-help.txt"
    );
}

#[test]
fn package_binary_and_release_tag_versions_match() {
    let package_version = env!("CARGO_PKG_VERSION");
    let prefix = format!("jeryu-tool-finder-v{package_version}-split.");
    let revision = release_tag()
        .strip_prefix(&prefix)
        .expect("VERSION package identity matches Cargo.toml");
    assert!(
        !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit()),
        "VERSION has a numeric split revision"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_jeryu-tool-finder"))
        .arg("--version")
        .output()
        .expect("run jeryu-tool-finder --version");
    assert!(output.status.success(), "--version failed");
    assert!(output.stderr.is_empty(), "--version wrote to stderr");
    assert_eq!(
        String::from_utf8(output.stdout).expect("version is UTF-8"),
        format!("jeryu-tool-finder {package_version}\n")
    );
}
