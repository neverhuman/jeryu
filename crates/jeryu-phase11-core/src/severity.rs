//! Severity levels and derived system health state.

/// Severity level for health, policy, and replay findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Degraded,
    Critical,
    Blocked,
}

impl Severity {
    /// Returns the stable lowercase representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Degraded => "degraded",
            Severity::Critical => "critical",
            Severity::Blocked => "blocked",
        }
    }

    /// Returns true when the severity should fail closed.
    pub fn fails_closed(self) -> bool {
        matches!(self, Severity::Critical | Severity::Blocked)
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// System health state used by operations automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Failing,
    Blocked,
}

impl HealthState {
    /// Derive a state from the highest severity in a report.
    pub fn from_severity(severity: Severity) -> Self {
        match severity {
            Severity::Info | Severity::Warning => HealthState::Healthy,
            Severity::Degraded => HealthState::Degraded,
            Severity::Critical => HealthState::Failing,
            Severity::Blocked => HealthState::Blocked,
        }
    }

    /// Stable representation.
    pub fn as_str(self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Degraded => "degraded",
            HealthState::Failing => "failing",
            HealthState::Blocked => "blocked",
        }
    }
}
