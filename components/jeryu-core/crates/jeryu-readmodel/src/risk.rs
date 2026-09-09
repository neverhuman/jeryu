//! Risk classification for actions surfaced in the read model.
//!
//! Ported into the contract crate so the read model carries no dependency on
//! the TUI rendering layer. The serde shape (lowercase `r0`..`r5` plus the
//! legacy aliases) is the wire contract and must remain stable.

use serde::{Deserialize, Serialize};

/// Six-tier risk classification.
///
/// Legacy four-tier `snake_case` names (`read_only`, `low`, `high`,
/// `production`) continue to deserialize through `#[serde(alias = ...)]` so
/// existing JSON fixtures keep working.
///
/// - R0: read-only (no side effects)
/// - R1: local mutation (jeryu-local state only)
/// - R2: CI mutation (triggers ci runs / requeues)
/// - R3: repo write (git branches / pull requests)
/// - R4: release / merge to production-adjacent ref
/// - R5: destructive, secret, or production-rooted (reserved; not yet used)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    #[serde(alias = "read_only")]
    R0,
    #[serde(alias = "low")]
    R1,
    R2,
    #[serde(alias = "high")]
    R3,
    #[serde(alias = "production")]
    R4,
    R5,
}

impl RiskTier {
    /// Short human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::R0 => "R0 (read)",
            Self::R1 => "R1 (local)",
            Self::R2 => "R2 (ci)",
            Self::R3 => "R3 (repo)",
            Self::R4 => "R4 (release)",
            Self::R5 => "R5 (destructive)",
        }
    }

    /// True when the action mutates state (anything above read-only).
    pub fn is_mutating(self) -> bool {
        !matches!(self, Self::R0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_lowercase() {
        assert_eq!(serde_json::to_string(&RiskTier::R3).unwrap(), "\"r3\"");
    }

    #[test]
    fn legacy_aliases_deserialize() {
        let cases = [
            ("\"read_only\"", RiskTier::R0),
            ("\"low\"", RiskTier::R1),
            ("\"high\"", RiskTier::R3),
            ("\"production\"", RiskTier::R4),
            ("\"r5\"", RiskTier::R5),
        ];
        for (json, expected) in cases {
            let got: RiskTier = serde_json::from_str(json).unwrap();
            assert_eq!(got, expected, "decoding {json}");
        }
    }

    #[test]
    fn mutating_classification() {
        assert!(!RiskTier::R0.is_mutating());
        assert!(RiskTier::R1.is_mutating());
        assert!(RiskTier::R4.is_mutating());
    }
}
