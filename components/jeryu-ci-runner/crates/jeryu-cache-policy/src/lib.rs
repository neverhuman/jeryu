mod fingerprint;
mod plan;
mod types;

#[cfg(test)]
mod tests;

pub use fingerprint::{FingerprintInputs, fingerprint_job};
pub use plan::{
    assert_cross_project_compiled_allowed, assert_release_cache_safe, plan_cache_for_job,
};
pub use types::{CacheDecision, CachePlan, CachePolicyError, CacheScope};
