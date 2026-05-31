//! Cache domain: integrity self-test report plus the [`InMemoryClient`]
//! implementation of the cache surface of [`ForgeClient`].

use serde::{Deserialize, Serialize};

use super::{ClientResult, InMemoryClient};

/// A cache integrity self-test report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheSelfTest {
    /// Number of probes executed.
    pub probes: u32,
    /// Number of false hits detected (must be zero to pass).
    pub false_hits: u32,
    /// Whether the integrity self-test passed.
    pub passed: bool,
}

// ---------------------------------------------------------------------------
// In-memory implementation: cache surface
// ---------------------------------------------------------------------------

impl InMemoryClient {
    pub(super) fn cache_self_test_inner(&self) -> ClientResult<CacheSelfTest> {
        Ok(CacheSelfTest {
            probes: 8,
            false_hits: 0,
            passed: true,
        })
    }
}
