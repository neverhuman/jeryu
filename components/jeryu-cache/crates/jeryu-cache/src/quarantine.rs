//! Quarantine reason types.

use std::fmt::{Display, Formatter};

/// Why a cache entry is quarantined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuarantineReason {
    /// Waiting for protected proof/promotion.
    PendingProof,
    /// Poison or false-hit suspicion.
    PoisonSuspected,
    /// Writer had insufficient trust for direct promotion.
    UntrustedWriter,
    /// Promotion failed policy.
    FailedPromotion,
}

impl Display for QuarantineReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            QuarantineReason::PendingProof => "pending-proof",
            QuarantineReason::PoisonSuspected => "poison-suspected",
            QuarantineReason::UntrustedWriter => "untrusted-writer",
            QuarantineReason::FailedPromotion => "failed-promotion",
        };
        f.write_str(value)
    }
}
