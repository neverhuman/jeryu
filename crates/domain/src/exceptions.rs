use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairHint {
    pub purpose: &'static str,
    pub reason: String,
    pub common_fixes: Vec<&'static str>,
    pub docs_url: &'static str,
    pub repair_hint: String,
}

impl RepairHint {
    pub fn new(
        purpose: &'static str,
        reason: impl Into<String>,
        common_fixes: Vec<&'static str>,
        docs_url: &'static str,
        repair_hint: impl Into<String>,
    ) -> Self {
        Self {
            purpose,
            reason: reason.into(),
            common_fixes,
            docs_url,
            repair_hint: repair_hint.into(),
        }
    }

    pub fn is_agent_actionable(&self) -> bool {
        !self.purpose.trim().is_empty()
            && !self.reason.trim().is_empty()
            && !self.common_fixes.is_empty()
            && !self.docs_url.trim().is_empty()
            && !self.repair_hint.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainFailure {
    pub code: &'static str,
    pub hint: RepairHint,
}

impl DomainFailure {
    pub fn new(code: &'static str, hint: RepairHint) -> Self {
        Self { code, hint }
    }
}

impl fmt::Display for DomainFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}. Rerun: {}",
            self.code, self.hint.reason, self.hint.repair_hint
        )
    }
}

impl Error for DomainFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_hint_requires_agent_action_fields() {
        let hint = RepairHint::new(
            "protect release gate invariants",
            "release-ready receipt is missing",
            vec!["run the release-ready lane", "inspect the receipt id"],
            "docs/testing.md#typed-repair-hint",
            "bash ops/ci/release-ready-lane.sh",
        );

        assert!(hint.is_agent_actionable());
    }

    #[test]
    fn domain_failure_display_names_narrow_rerun() {
        let failure = DomainFailure::new(
            "release.receipt_missing",
            RepairHint::new(
                "protect release gate invariants",
                "release-ready receipt is missing",
                vec!["run the release-ready lane"],
                "docs/testing.md#typed-repair-hint",
                "bash ops/ci/release-ready-lane.sh",
            ),
        );

        assert_eq!(
            failure.to_string(),
            "release.receipt_missing: release-ready receipt is missing. Rerun: bash ops/ci/release-ready-lane.sh"
        );
    }
}
