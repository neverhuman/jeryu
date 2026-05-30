//! False-hit detection.

use crate::cache::CacheEntry;
use crate::digest::{digest_bytes, Digest};

/// Result of a false-hit verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalseHitReport {
    /// True when the object and manifest match expectations.
    pub clean: bool,
    /// Explanation of any mismatch.
    pub reason: String,
}

/// Stateless false-hit detector.
#[derive(Debug, Default, Clone, Copy)]
pub struct FalseHitDetector;

impl FalseHitDetector {
    /// Verifies restored bytes and expected manifest digest against an index entry.
    pub fn verify(
        entry: &CacheEntry,
        restored_bytes: &[u8],
        expected_manifest_digest: &Digest,
    ) -> FalseHitReport {
        let actual_object_digest = digest_bytes(restored_bytes);
        if actual_object_digest != entry.object_digest {
            return FalseHitReport {
                clean: false,
                reason: format!(
                    "object digest mismatch: index={} actual={}",
                    entry.object_digest, actual_object_digest
                ),
            };
        }
        if expected_manifest_digest != &entry.manifest_digest {
            return FalseHitReport {
                clean: false,
                reason: format!(
                    "manifest digest mismatch: index={} expected={}",
                    entry.manifest_digest, expected_manifest_digest
                ),
            };
        }
        FalseHitReport {
            clean: true,
            reason: "object and manifest match".to_string(),
        }
    }
}
