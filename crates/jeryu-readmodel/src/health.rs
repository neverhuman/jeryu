//! Component / runner health used by the System Health and Source Doctor
//! surfaces. Component names are provider-neutral (`scm`, not a vendor name).

use serde::{Deserialize, Serialize};

use crate::entity::HealthLevel;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthLevel,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

impl ComponentHealth {
    pub fn unknown(name: &str) -> Self {
        Self {
            name: name.into(),
            status: HealthLevel::Degraded,
            latency_ms: None,
            detail: Some("not yet checked".into()),
        }
    }

    pub fn ok(name: &str, latency_ms: u64) -> Self {
        Self {
            name: name.into(),
            status: HealthLevel::Healthy,
            latency_ms: Some(latency_ms),
            detail: None,
        }
    }

    /// Human-readable status label for display.
    pub fn status_label(&self) -> String {
        match self.status {
            HealthLevel::Healthy => "healthy".to_string(),
            HealthLevel::Warning => "warning".to_string(),
            HealthLevel::Degraded => "degraded".to_string(),
            HealthLevel::Critical => "critical".to_string(),
            HealthLevel::Unknown => "unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RunnerHealth {
    pub online: u32,
    pub busy: u32,
    pub idle: u32,
    pub degraded: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_health_ok_reports_latency() {
        let h = ComponentHealth::ok("scm", 12);
        assert_eq!(h.latency_ms, Some(12));
        assert!(matches!(h.status, HealthLevel::Healthy));
        assert_eq!(h.status_label(), "healthy");
    }

    #[test]
    fn component_health_unknown_is_degraded() {
        let h = ComponentHealth::unknown("vault");
        assert!(matches!(h.status, HealthLevel::Degraded));
    }
}
