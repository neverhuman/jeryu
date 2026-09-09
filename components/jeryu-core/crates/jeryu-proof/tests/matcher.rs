//! Integration tests for the path matcher used by owner / test / generated maps.
//!
//! These cover the four documented matching modes: universal (`*` / `**`),
//! recursive subtree (`prefix/**`), bare-suffix prefix (`prefix*`), and exact
//! equality. The matcher is glob-style only; there is no regex backend, so the
//! tests assert the literal glob semantics the engine actually relies on.

use jeryu_proof::PathPattern;

#[test]
fn universal_double_star_matches_everything() {
    let p = PathPattern::new("**");
    assert!(p.matches(""));
    assert!(p.matches("anything"));
    assert!(p.matches("deeply/nested/path/file.rs"));
    assert!(p.matches("Cargo.toml"));
}

#[test]
fn universal_single_star_matches_everything() {
    // A lone `*` is treated as universal, identical to `**`.
    let p = PathPattern::new("*");
    assert!(p.matches(""));
    assert!(p.matches("a"));
    assert!(p.matches("crates/jeryu_core/src/lib.rs"));
}

#[test]
fn double_star_subtree_matches_root_and_descendants() {
    let p = PathPattern::new("crates/jeryu_core/**");
    // The prefix itself (the directory node) matches.
    assert!(p.matches("crates/jeryu_core"));
    // Direct children match.
    assert!(p.matches("crates/jeryu_core/Cargo.toml"));
    // Deep descendants match.
    assert!(p.matches("crates/jeryu_core/src/phase7.rs"));
}

#[test]
fn double_star_subtree_rejects_sibling_and_prefix_collisions() {
    let p = PathPattern::new("crates/jeryu_core/**");
    // A sibling crate must not match.
    assert!(!p.matches("crates/jeryu_proof/src/lib.rs"));
    // A path that merely shares the textual prefix but is a different
    // directory (no `/` boundary) must NOT match. This is the key guard that
    // distinguishes `prefix/**` from a naive `starts_with`.
    assert!(!p.matches("crates/jeryu_core_extra/src/lib.rs"));
    // A shorter unrelated path must not match.
    assert!(!p.matches("crates"));
}

#[test]
fn double_star_subtree_requires_separator_boundary() {
    let p = PathPattern::new("agent/**");
    assert!(p.matches("agent"));
    assert!(p.matches("agent/policies.toml"));
    // "agentbridge" shares the prefix but crosses no separator -> reject.
    assert!(!p.matches("agentbridge"));
    assert!(!p.matches("agentbridge/file.rs"));
}

#[test]
fn bare_suffix_star_is_prefix_match() {
    // A trailing `*` that is NOT preceded by `/**` is a plain prefix match,
    // and importantly does NOT require a separator boundary.
    let p = PathPattern::new("crates/jeryu_proof*");
    assert!(p.matches("crates/jeryu_proof"));
    assert!(p.matches("crates/jeryu_proof_extra"));
    assert!(p.matches("crates/jeryu_proof/src/lib.rs"));
    assert!(!p.matches("crates/jeryu_core"));
}

#[test]
fn exact_pattern_matches_only_the_exact_path() {
    let p = PathPattern::new("Cargo.toml");
    assert!(p.matches("Cargo.toml"));
    assert!(!p.matches("crates/jeryu_core/Cargo.toml"));
    assert!(!p.matches("Cargo.tomlx"));
    assert!(!p.matches("Cargo.tom"));
    assert!(!p.matches(""));
}

#[test]
fn exact_pattern_is_case_sensitive() {
    let p = PathPattern::new("AGENTS.md");
    assert!(p.matches("AGENTS.md"));
    assert!(!p.matches("agents.md"));
    assert!(!p.matches("Agents.md"));
}

#[test]
fn as_str_round_trips_the_pattern_text() {
    let p = PathPattern::new("ops/ci/**");
    assert_eq!(p.as_str(), "ops/ci/**");
}

#[test]
fn pattern_equality_and_clone_are_value_based() {
    let a = PathPattern::new("docs/**");
    let b = a.clone();
    assert_eq!(a, b);
    assert_ne!(a, PathPattern::new("docs/api/**"));
}
