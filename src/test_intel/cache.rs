//! Owner: VTI Test Intelligence subsystem — plan cache
//! Proof: `cargo nextest run -p jeryu -- test_intel::cache`
//! Invariants: Cached plans are keyed by inputs that affect test selection and expire on outdated evidence.
//! Test result caching via content-addressed witness hashes.
//!
//! This module enables 20-100x CI speedups for repeat runs by caching test
//! verdicts keyed on a SHA-256 of all relevant inputs: source file hashes,
//! Cargo.lock hash, rustc version, and cache epoch.
//!
//! Cacheability rules:
//! - Unit tests with no external deps: **cacheable**
//! - Integration tests touching Docker/GitLab: **not cacheable**
//! - E2E tests: **never cacheable**
//! - Tests with flake history: **never cacheable**

use sha2::{Digest, Sha256};

#[path = "cache_types.rs"]
mod types;
pub use types::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Compute a deterministic cache key for a test execution.
///
/// The key is a SHA-256 of:
/// - test_id (the command string)
/// - source file content hashes (sorted by path for determinism)
/// - Cargo.lock hash
/// - rustc version
/// - cache epoch (allows global invalidation)
pub fn compute_cache_key(
    test_id: &str,
    source_hashes: &[(&str, &str)], // (path, content_hash)
    cargo_lock_hash: &str,
    rustc_version: &str,
    cache_epoch: i64,
) -> TestCacheKey {
    let mut hasher = Sha256::new();
    let mut uncacheable_reasons = Vec::new();

    // 1. Test identity
    hasher.update(b"test:");
    hasher.update(test_id.as_bytes());
    hasher.update(b"\n");

    // 2. Source hashes (sorted for determinism)
    let mut sorted_hashes: Vec<_> = source_hashes.to_vec();
    sorted_hashes.sort_by_key(|(path, _)| path.to_string());
    for (path, hash) in &sorted_hashes {
        hasher.update(b"src:");
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }

    // 3. Cargo.lock
    hasher.update(b"lock:");
    hasher.update(cargo_lock_hash.as_bytes());
    hasher.update(b"\n");

    // 4. Toolchain
    hasher.update(b"rustc:");
    hasher.update(rustc_version.as_bytes());
    hasher.update(b"\n");

    // 5. Cache epoch
    hasher.update(b"epoch:");
    hasher.update(cache_epoch.to_string().as_bytes());
    hasher.update(b"\n");

    let digest = hex::encode(hasher.finalize());

    // Classify cacheability
    let cacheability = classify_cacheability(test_id, &mut uncacheable_reasons);

    let inputs_desc = format!(
        "test={}, sources={}, lock={}, rustc={}, epoch={}",
        test_id,
        sorted_hashes.len(),
        &cargo_lock_hash[..8.min(cargo_lock_hash.len())],
        rustc_version,
        cache_epoch
    );

    TestCacheKey {
        digest,
        inputs_description: inputs_desc,
        cacheability,
        uncacheable_reasons,
    }
}

/// Classify whether a test command is cacheable based on its name/type.
fn classify_cacheability(test_id: &str, reasons: &mut Vec<String>) -> Cacheability {
    let id_lower = test_id.to_lowercase();

    // E2E tests: never cacheable (external state)
    if id_lower.contains("e2e")
        || id_lower.contains("end_to_end")
        || id_lower.contains("end-to-end")
    {
        reasons.push("E2E tests depend on external state and are never cacheable".into());
        return Cacheability::Uncacheable;
    }

    // Docker/container tests: not cacheable
    if id_lower.contains("docker") || id_lower.contains("container") || id_lower.contains("dind") {
        reasons.push("Docker/container tests depend on daemon state".into());
        return Cacheability::Uncacheable;
    }

    // GitLab API tests: not cacheable
    if id_lower.contains("gitlab") && id_lower.contains("live") {
        reasons.push("Live GitLab API tests depend on external service".into());
        return Cacheability::Uncacheable;
    }

    // Integration tests that explicitly use network: not cacheable
    if id_lower.contains("--test")
        && (id_lower.contains("pool_tests") || id_lower.contains("job_tests"))
    {
        reasons.push("Pool/job integration tests may depend on Docker daemon".into());
        return Cacheability::Uncacheable;
    }

    // Agent tests: may use network
    if id_lower.contains("--test") && id_lower.contains("agent_tests") {
        reasons.push("Agent integration tests may use network resources".into());
        return Cacheability::Uncacheable;
    }

    // Unit tests (--lib, nextest -E 'test(...)') are cacheable
    Cacheability::Cacheable
}

/// Mark a test as flaky-uncacheable.
pub fn mark_flaky(key: &mut TestCacheKey) {
    key.cacheability = Cacheability::FlakyUncacheable;
    key.uncacheable_reasons
        .push("Test has flake history — cache disabled".into());
}

#[path = "cache_lookup.rs"]
mod lookup;
pub use lookup::*;

#[cfg(test)]
pub(crate) fn format_duration_ms(ms: u64) -> String {
    lookup::format_duration_ms(ms)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
