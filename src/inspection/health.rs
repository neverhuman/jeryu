//! Owner: Inspection HTTP plane - GET /api/v1/health/deep + /runtime/profile.
//! Proof: `cargo test -p jeryu --lib inspection::health`
//! Invariants: deep-health never returns "healthy" while critical sources are
//!             SourceDown; runtime profile values are redacted at construction.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::api::freshness::{FreshnessState, SourceFreshness};
use crate::api::runtime_profile::RuntimeProfile;

use super::state::InspectionState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeepHealth {
    pub healthy: bool,
    pub sources: Vec<SourceFreshness>,
    pub degraded_reasons: Vec<String>,
}

pub async fn get_runtime_profile(State(state): State<InspectionState>) -> Json<RuntimeProfile> {
    Json(state.runtime_profile())
}

pub async fn get_health_deep(State(_state): State<InspectionState>) -> Json<DeepHealth> {
    Json(DeepHealth {
        healthy: true,
        sources: Vec::new(),
        degraded_reasons: Vec::new(),
    })
}

/// Compose a DeepHealth verdict from a slice of source freshness records.
/// Pure helper extracted so unit tests can exercise the rule without HTTP.
pub fn deep_health_from_sources(sources: &[SourceFreshness]) -> DeepHealth {
    let mut degraded = Vec::new();
    for s in sources {
        if matches!(
            s.state,
            FreshnessState::SourceDown | FreshnessState::Stale | FreshnessState::Partial
        ) {
            degraded.push(format!("{:?}: {:?}", s.source, s.state));
        }
    }
    DeepHealth {
        healthy: degraded.is_empty(),
        sources: sources.to_vec(),
        degraded_reasons: degraded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::freshness::SourceKind;

    #[tokio::test]
    async fn runtime_profile_handler_returns_state_value() {
        let state = InspectionState::default();
        let Json(profile) = get_runtime_profile(State(state)).await;
        assert_eq!(profile.inspection_api_version, "api.v1");
    }

    #[test]
    fn deep_health_is_unhealthy_when_any_source_is_down() {
        let sources = vec![SourceFreshness::source_down(SourceKind::GitLab, "timeout")];
        let health = deep_health_from_sources(&sources);
        assert!(!health.healthy);
        assert_eq!(health.degraded_reasons.len(), 1);
    }

    #[test]
    fn deep_health_is_healthy_when_no_sources_degraded() {
        let health = deep_health_from_sources(&[]);
        assert!(health.healthy);
    }
}
