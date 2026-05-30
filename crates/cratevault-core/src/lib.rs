//! CrateVault core types and cache-law enforcement.
//!
//! The crate deliberately keeps product truth in typed Rust structures: cache
//! layers, trust tiers, cache key material, policy decisions, receipts, CAS
//! writes, quarantine, and verification are all explicit and testable.

pub mod cas;
pub mod digest;
pub mod error;
pub mod key;
pub mod manifest;
pub mod policy;
pub mod quarantine;
pub mod receipt;
pub mod tier;
pub mod verify;

pub use cas::{CasObject, ContentAddressedStore};
pub use digest::Digest;
pub use error::{CrateVaultError, Result};
pub use key::{CacheKey, CacheKeyMaterial};
pub use manifest::{CacheEntry, CacheManifest};
pub use policy::{AccessDecision, CacheAction, CacheLayer, CacheRequest, CacheScope, PolicyEngine};
pub use quarantine::{QuarantineRecord, QuarantineStore};
pub use receipt::{CacheEvent, CacheReceipt, ReceiptSink};
pub use tier::TrustTier;
pub use verify::{verify_cache_hit, FalseHit, VerificationReport};
