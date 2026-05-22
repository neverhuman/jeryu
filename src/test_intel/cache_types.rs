use serde::{Deserialize, Serialize};

/// Cacheability classification for a test command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cacheability {
    /// Fully cacheable; can skip if cache hit
    Cacheable,
    /// Not cacheable due to external dependencies
    Uncacheable,
    /// Forced uncacheable due to flakiness history
    FlakyUncacheable,
}

/// A cache key for a specific test execution context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCacheKey {
    /// The computed SHA-256 digest
    pub digest: String,
    /// Human-readable description of inputs
    pub inputs_description: String,
    /// Whether this key represents a cacheable result
    pub cacheability: Cacheability,
    /// Reasons why it's uncacheable (if applicable)
    pub uncacheable_reasons: Vec<String>,
}

/// A cached test verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedVerdict {
    /// The cache key that produced this verdict
    pub cache_key: String,
    /// The test identifier
    pub test_id: String,
    /// Pass or fail
    pub passed: bool,
    /// Duration of the original run in milliseconds
    pub duration_ms: u64,
    /// When this was cached
    pub cached_at: String,
    /// Cache epoch at time of caching
    pub epoch: i64,
}

/// Result of checking the cache for a set of tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLookupResult {
    /// Tests that had a cache hit (can be skipped)
    pub hits: Vec<CacheHit>,
    /// Tests that need to be re-run
    pub misses: Vec<CacheMiss>,
    /// Total time saved by cache hits (ms)
    pub time_saved_ms: u64,
    /// Summary statistics
    pub hit_rate: f64,
}

/// A single cache hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheHit {
    pub test_id: String,
    pub cache_key: String,
    pub original_duration_ms: u64,
    pub cached_at: String,
}

/// A single cache miss.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMiss {
    pub test_id: String,
    pub reason: String,
}
