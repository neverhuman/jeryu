//! Upgrade and rollback plans.

/// Semantic-ish version used by upgrade drills.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    /// Construct a version.
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Stable string.
    pub fn as_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Upgrade plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradePlan {
    pub from: Version,
    pub to: Version,
    pub preflight_checks: Vec<String>,
    pub migration_steps: Vec<String>,
}

/// Rollback plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RollbackPlan {
    pub upgrade: UpgradePlan,
    pub rollback_steps: Vec<String>,
    pub invariant_checks: Vec<String>,
}

impl RollbackPlan {
    /// Build the Phase 10 standard rollback drill.
    pub fn phase10() -> Self {
        Self {
            upgrade: UpgradePlan {
                from: Version::new(0, 9, 0),
                to: Version::new(0, 10, 0),
                preflight_checks: vec![
                    "backup-complete".to_owned(),
                    "audit-chain-valid".to_owned(),
                    "slo-green".to_owned(),
                ],
                migration_steps: vec![
                    "schema-add-phase10".to_owned(),
                    "backfill-rbac-decisions".to_owned(),
                    "publish-scorecard".to_owned(),
                ],
            },
            rollback_steps: vec![
                "stop-api-writes".to_owned(),
                "restore-schema-snapshot".to_owned(),
                "replay-audit-outbox".to_owned(),
                "resume-api".to_owned(),
            ],
            invariant_checks: vec![
                "no-receipt-loss".to_owned(),
                "tenant-boundaries-intact".to_owned(),
                "sso-configs-valid".to_owned(),
            ],
        }
    }

    /// True when the plan has enough evidence to run safely.
    pub fn is_reversible(&self) -> bool {
        self.upgrade.to > self.upgrade.from
            && !self.upgrade.preflight_checks.is_empty()
            && !self.upgrade.migration_steps.is_empty()
            && self.rollback_steps.len() >= 3
            && self.invariant_checks.len() >= 3
    }
}
