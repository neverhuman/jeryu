//! Behavioral tests for the zero-evidence guard.
//!
//! Forbidden tokens are injected via hex-decoded bytes so this test source
//! never contains the literal markers (preserving the self-clean property).

use std::fs;
use std::path::Path;

use jeryu_evidence::scan;
use tempfile::TempDir;

/// Decode a hex marker into its raw bytes for fixture injection.
fn marker(hexed: &str) -> Vec<u8> {
    hex::decode(hexed).expect("valid hex marker")
}

#[test]
fn clean_tree_passes() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(
        dir.path().join("a.txt"),
        b"a clean line\nanother clean line\n",
    )
    .expect("write a");
    fs::create_dir(dir.path().join("sub")).expect("mkdir sub");
    fs::write(dir.path().join("sub/b.rs"), b"fn main() {}\n").expect("write b");

    let findings = scan(dir.path()).expect("scan ok");
    assert!(
        findings.is_empty(),
        "clean tree produced findings: {findings:?}"
    );
}

#[test]
fn injected_forbidden_token_fails() {
    let dir = TempDir::new().expect("tempdir");
    // "6769746c6162" decodes to a forbidden third-party forge brand literal.
    let mut content = b"first line is fine\nsecond line has ".to_vec();
    content.extend_from_slice(&marker("6769746c6162"));
    content.extend_from_slice(b" embedded\n");
    fs::write(dir.path().join("bad.txt"), &content).expect("write bad");

    let findings = scan(dir.path()).expect("scan ok");
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one finding: {findings:?}"
    );
    assert_eq!(findings[0].rel, Path::new("bad.txt"));
    assert_eq!(findings[0].line, 2, "marker is on the second line");
    assert_eq!(findings[0].to_string(), "bad.txt:2: blocked marker");
}

#[test]
fn agent_chat_is_ignored_but_ci_tracker_still_reports() {
    let dir = TempDir::new().expect("tempdir");
    let mut ignored = b"coordination log\n".to_vec();
    ignored.extend_from_slice(&marker("6769746c6162"));
    ignored.extend_from_slice(b" preserved\n");
    fs::write(dir.path().join("AGENT_CHAT.md"), &ignored).expect("write agent chat");

    let mut tracked = b"confidence ledger\n".to_vec();
    tracked.extend_from_slice(&marker("6769746c6162"));
    tracked.extend_from_slice(b" remains strict\n");
    fs::write(dir.path().join("CI_TRACKER.md"), &tracked).expect("write ci tracker");

    let findings = scan(dir.path()).expect("scan ok");
    assert_eq!(findings.len(), 1, "only CI_TRACKER.md should report");
    assert_eq!(findings[0].rel, Path::new("CI_TRACKER.md"));
    assert_eq!(findings[0].line, 2);
    assert_eq!(findings[0].to_string(), "CI_TRACKER.md:2: blocked marker");
}

#[test]
fn matching_is_case_insensitive() {
    let dir = TempDir::new().expect("tempdir");
    // Uppercase the decoded literal; the scanner lowercases content before
    // matching, so an uppercase occurrence must still be caught.
    let upper: Vec<u8> = marker("6a6974666f726765")
        .iter()
        .map(u8::to_ascii_uppercase)
        .collect();
    let mut content = b"prefix ".to_vec();
    content.extend_from_slice(&upper);
    fs::write(dir.path().join("upper.txt"), &content).expect("write upper");

    let findings = scan(dir.path()).expect("scan ok");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].line, 1);
}

#[test]
fn skip_dirs_are_ignored() {
    let dir = TempDir::new().expect("tempdir");
    // A forbidden token buried under a skipped directory must not be reported.
    for skip in [
        ".git",
        "target",
        ".jankurai",
        "node_modules",
        "dist",
        "playwright-report",
        ".worktrees",
    ] {
        let sub = dir.path().join(skip);
        fs::create_dir_all(&sub).expect("mkdir skip");
        let mut content = b"junk ".to_vec();
        content.extend_from_slice(&marker("6e6974726f"));
        fs::write(sub.join("planted.txt"), &content).expect("write planted");
    }

    let findings = scan(dir.path()).expect("scan ok");
    assert!(
        findings.is_empty(),
        "forbidden tokens in skip dirs leaked: {findings:?}"
    );
}

#[test]
fn generated_typescript_build_info_is_ignored() {
    let dir = TempDir::new().expect("tempdir");
    let mut content = b"compiler cache ".to_vec();
    content.extend_from_slice(&marker("6769746c6162"));
    fs::write(dir.path().join("tsconfig.tsbuildinfo"), &content).expect("write buildinfo");

    let findings = scan(dir.path()).expect("scan ok");
    assert!(
        findings.is_empty(),
        "generated build info leaked into scan: {findings:?}"
    );
}

#[test]
fn first_marker_wins_one_finding_per_file() {
    let dir = TempDir::new().expect("tempdir");
    // Two distinct markers in one file yield a single finding: scanning of a
    // file stops at the first match.
    let mut content = Vec::new();
    content.extend_from_slice(&marker("6769746c6162"));
    content.extend_from_slice(b"\n");
    content.extend_from_slice(&marker("6a6974666f726765"));
    fs::write(dir.path().join("two.txt"), &content).expect("write two");

    let findings = scan(dir.path()).expect("scan ok");
    assert_eq!(findings.len(), 1, "one finding per file: {findings:?}");
    assert_eq!(findings[0].line, 1, "first marker on line 1 wins");
}
