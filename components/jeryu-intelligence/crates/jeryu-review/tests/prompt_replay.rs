//! Verdict-replay gate: `prompt_sha` over each ported prompt asset must be
//! byte-stable. Replayed verdicts bind to these hashes, so a drift here would
//! silently invalidate every historical verdict. The pinned values are the
//! replay anchors for this crate's prompt set.

use jeryu_review::prompt_builder::prompt_sha;
use std::path::Path;

fn asset(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/prompts")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// (asset file, pinned prompt_sha). If a prompt is edited on a hashed line this
/// MUST be updated in the same change — and a verdict-replay rotation
/// documented.
const PINNED: &[(&str, &str)] = &[
    (
        "reviewer-security.md",
        "sha256:459a7b07c681e9195483ee415b9507500f431dbce37ad515424bcd0bff66d790",
    ),
    (
        "reviewer-test-integrity.md",
        "sha256:47c85d34cadf451d457f48d12005dd46c3e226eaaadbe3130fa748c4312ca4a1",
    ),
    (
        "reviewer-runtime.md",
        "sha256:429d9e8cc569d10b18dc1a0fe675552279585f16dfdebc91cbe7738758d08b6b",
    ),
    (
        "lockfile-scout.md",
        "sha256:39d2ad06a636324a9a7388b2c83bf2fc070bcb3b935e23152b19ca6ea8ed79f1",
    ),
    (
        "reviewer-nightwatch.md",
        "sha256:4b06bcea41c2aaa6a2e46be855d6ea5a6b86a2100bcb03dee6ef022b07256d25",
    ),
];

#[test]
fn prompt_sha_is_pinned_for_every_asset() {
    for (file, pinned) in PINNED {
        let md = asset(file);
        assert_eq!(
            &prompt_sha(&md),
            pinned,
            "prompt_sha drift for {file}: editing a hashed line rotates every historical verdict"
        );
    }
}

#[test]
fn prompt_sha_is_stable_across_reads() {
    for (file, _) in PINNED {
        let a = prompt_sha(&asset(file));
        let b = prompt_sha(&asset(file));
        assert_eq!(a, b, "prompt_sha must be deterministic for {file}");
    }
}

#[test]
fn no_hash_comment_does_not_affect_sha() {
    let base = "You are reviewer-x.v1.\n";
    let commented = "# (no-hash) edited today\nYou are reviewer-x.v1.\n";
    assert_eq!(prompt_sha(base), prompt_sha(commented));
}

#[test]
fn hashed_edit_rotates_sha() {
    assert_ne!(
        prompt_sha("You are reviewer-x.v1.\n"),
        prompt_sha("You are reviewer-x.v2.\n")
    );
}
