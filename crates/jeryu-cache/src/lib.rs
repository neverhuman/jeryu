//! JeryuCache: Phase 6 cache/CAS implementation for Jeryu.
//!
//! The crate is dependency-free and intentionally small, but the boundaries mirror
//! the production design: fingerprints, policy, CAS, receipts, quarantine,
//! promotion, and adversarial cache-law validation.

pub mod cache;
pub mod cas;
pub mod digest;
pub mod error;
pub mod false_hit;
pub mod fingerprint;
pub mod harness;
pub mod ids;
pub mod policy;
pub mod quarantine;
pub mod receipt;
pub mod service;

pub use cache::{CacheEntry, CacheEntryState, CacheKey, CacheKind, CacheScope};
pub use cas::{CasBackend, FsCas};
pub use digest::{Digest, digest_bytes};
pub use error::{Result, VaultError};
pub use fingerprint::FingerprintInput;
pub use ids::{Actor, RepoId, SharedScopeId, TenantId};
pub use policy::{CacheAccess, CacheContext, CacheDisposition, CachePolicy, TrustTier};
pub use receipt::{CacheAction, CacheReceipt, ReceiptLog};
pub use service::{JeryuCache, RestoreStatus, StoreStatus};
