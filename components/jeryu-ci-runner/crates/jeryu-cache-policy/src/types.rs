//! Cache policy scope, decision, plan, and error types.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheScope {
    JobTmpfs,
    RunnerLocalProject,
    RunnerLocalRegistry,
    RepoCompiledCas,
    TenantSourceCas,
    ExplicitSharedCompiledCas(String),
    ReleaseHermeticVendorSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheDecision {
    pub scope: CacheScope,
    pub read_allowed: bool,
    pub write_allowed: bool,
    pub promote_after_green: bool,
    pub quarantine: bool,
    pub mutable_compiled_cache_allowed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachePlan {
    pub job_id: String,
    pub decisions: Vec<CacheDecision>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CachePolicyError {
    CrossProjectCompiledDenied,
    ReleaseMutableCacheDenied,
    MissingFingerprintInput(String),
}

impl fmt::Display for CachePolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CrossProjectCompiledDenied => {
                f.write_str("cross-project compiled cache is denied by default")
            }
            Self::ReleaseMutableCacheDenied => {
                f.write_str("release lane cannot consume mutable compiled cache")
            }
            Self::MissingFingerprintInput(input) => {
                write!(f, "missing cache fingerprint input: {input}")
            }
        }
    }
}

impl std::error::Error for CachePolicyError {}
