#![forbid(unsafe_code)]
#![doc = "Shared Phase 11 domain types, deterministic hashing, and JSON helpers."]

mod ids;
mod json;
mod report;
mod severity;
mod validation;

pub use ids::{Digest, ExportFormat, TenantId, UpgradeRing, Version};
pub use json::{json_array, now_unix_seconds, quote};
pub use report::{Finding, PolicyDecision, Report};
pub use severity::{HealthState, Severity};
pub use validation::{ValidationError, validate_slug};

/// Phase 11 release marker.
pub const PHASE: u8 = 11;
/// Product name used in receipts.
pub const PRODUCT: &str = "Jeryu";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_rejects_bad_input() {
        assert_eq!(
            Version::parse("0.11.0")
                .unwrap_or_else(|_| Version::new(0, 0, 0))
                .to_string(),
            "0.11.0"
        );
        assert!(Version::parse("0.11").is_err());
        assert!(Version::parse("0.11.x").is_err());
    }

    #[test]
    fn report_fails_closed_for_blocked() {
        let mut report = Report::new("readiness");
        report.push(Finding::new(
            "tenant.broad",
            Severity::Blocked,
            "broad permission denied",
        ));
        assert!(report.fails_closed());
        assert!(report.to_json().contains("tenant.broad"));
    }

    #[test]
    fn quote_escapes_control_characters() {
        assert_eq!(quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
