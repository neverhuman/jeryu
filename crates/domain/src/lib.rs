//! Canonical domain repair surface.
//!
//! The durable implementation lives in `jeryu-core`; this crate gives local
//! agents and audit tools a stable domain route for typed, repairable errors.

pub use jeryu_core::{AgentRepairHint, JeryuError, JeryuResult};

pub const AGENT_FRIENDLY_EXCEPTION_REQUIRED_FIELDS: &[&str] = &[
    "purpose",
    "reason",
    "common_fixes",
    "docs_url",
    "repair_hint",
];

/// Owned domain exception pattern for agent-readable repair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainExceptionPattern {
    /// Purpose of the failed domain operation.
    pub purpose: &'static str,
    /// Reason the operation failed.
    pub reason: &'static str,
    /// Common fixes that can repair the failure locally.
    pub common_fixes: &'static [&'static str],
    /// Documentation URL for the failure class.
    pub docs_url: &'static str,
    /// Repair hint with the next rerun command.
    pub repair_hint: &'static str,
}

pub type AgentFriendlyExceptionPattern = DomainExceptionPattern;

/// Return the machine-readable repair hint for a domain error.
pub fn repair_hint(error: &JeryuError) -> AgentRepairHint {
    error.agent_repair_hint()
}

/// Return the canonical exception pattern expected by agents.
pub fn exception_pattern() -> DomainExceptionPattern {
    DomainExceptionPattern {
        purpose: "route domain errors to local repair",
        reason: "domain operation requires typed evidence before retry",
        common_fixes: &[
            "inspect the structured domain error",
            "run the mapped proof lane before retrying mutation",
        ],
        docs_url: "docs/errors.md",
        repair_hint: "rerun cargo test -p jeryu-domain --jobs 40",
    }
}

/// Return the explicitly named agent-friendly exception pattern for audits.
pub fn agent_friendly_exception_pattern() -> AgentFriendlyExceptionPattern {
    exception_pattern()
}

/// Return the docs URL for the agent-friendly exception contract.
pub fn agent_friendly_exception_docs_url() -> &'static str {
    exception_pattern().docs_url
}

/// Return the common local fixes for the agent-friendly exception contract.
pub fn agent_friendly_exception_common_fixes() -> &'static [&'static str] {
    exception_pattern().common_fixes
}

#[cfg(test)]
mod tests {
    use super::{
        AGENT_FRIENDLY_EXCEPTION_REQUIRED_FIELDS, JeryuError,
        agent_friendly_exception_common_fixes, agent_friendly_exception_docs_url,
        agent_friendly_exception_pattern, exception_pattern, repair_hint,
    };

    #[test]
    fn domain_errors_expose_agent_repair_hints() {
        let hint = repair_hint(&JeryuError::PolicyDenied("branch protection".to_string()));
        assert_eq!(hint.reason, "policy_denied");
        assert_eq!(hint.docs_url, "docs/errors.md#policy-denied");
        assert!(hint.common_fixes.iter().any(|fix| fix.contains("receipt")));
        assert!(hint.repair_hint.contains("rerun"));
    }

    #[test]
    fn domain_exception_pattern_names_repair_contract_fields() {
        let pattern = exception_pattern();
        assert_eq!(pattern.purpose, "route domain errors to local repair");
        assert!(pattern.reason.contains("typed evidence"));
        assert!(!pattern.common_fixes.is_empty());
        assert_eq!(pattern.docs_url, "docs/errors.md");
        assert!(pattern.repair_hint.contains("jeryu-domain"));
    }

    #[test]
    fn agent_friendly_exception_pattern_exposes_required_fields() {
        assert_eq!(
            AGENT_FRIENDLY_EXCEPTION_REQUIRED_FIELDS,
            [
                "purpose",
                "reason",
                "common_fixes",
                "docs_url",
                "repair_hint"
            ]
        );
        assert_eq!(agent_friendly_exception_pattern(), exception_pattern());
        assert_eq!(agent_friendly_exception_docs_url(), "docs/errors.md");
        assert!(
            agent_friendly_exception_common_fixes()
                .iter()
                .any(|fix| fix.contains("proof lane"))
        );
    }
}
