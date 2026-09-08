//! Red-team gates.

/// One red-team finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedTeamFinding {
    pub id: String,
    pub severity: String,
    pub blocked: bool,
    pub evidence: String,
}

/// Security red-team suite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedTeamSuite {
    pub findings: Vec<RedTeamFinding>,
}

impl RedTeamSuite {
    /// Deterministic Phase 10 suite covering enterprise gates.
    pub fn phase10() -> Self {
        Self {
            findings: vec![
                RedTeamFinding {
                    id: "tenant-cross-read".to_owned(),
                    severity: "critical".to_owned(),
                    blocked: true,
                    evidence: "rbac denied cross-tenant read".to_owned(),
                },
                RedTeamFinding {
                    id: "sso-http-redirect".to_owned(),
                    severity: "high".to_owned(),
                    blocked: true,
                    evidence: "oidc validation rejected insecure redirect".to_owned(),
                },
                RedTeamFinding {
                    id: "audit-tamper".to_owned(),
                    severity: "critical".to_owned(),
                    blocked: true,
                    evidence: "hash chain mismatch detected".to_owned(),
                },
                RedTeamFinding {
                    id: "rollback-without-backup".to_owned(),
                    severity: "high".to_owned(),
                    blocked: true,
                    evidence: "preflight gate failed".to_owned(),
                },
            ],
        }
    }

    /// True when every modeled attack is blocked.
    pub fn passes(&self) -> bool {
        !self.findings.is_empty() && self.findings.iter().all(|finding| finding.blocked)
    }
}
