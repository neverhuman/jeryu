//! Phase-1 audit hook (W-CC-05).
//!
//! Real audit writes target the `audit_events` and `web_action_receipts`
//! tables (see §35.1.16); the Phase-1 shell here just emits a structured
//! tracing event so traces are searchable while the durable-row plumbing
//! lands with the service-layer work packages.

use serde_json::Value;
use tracing::info;

/// Risk tiers used by the §35.1.14 action-safety pipeline. `Low` actions
/// (read-only / metadata) skip preview; `Medium` actions show a preview;
/// `High` actions additionally require expected-SHA / expected-hash
/// concurrency tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskTier {
    Low,
    Medium,
    High,
}

impl RiskTier {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Phase-1 audit write: emit a tracing event tagged with the canonical
/// audit fields. The real implementation (W-B-* service layer) writes to
/// `audit_events`/`web_action_receipts` in the same transaction as the
/// business mutation per §35.1.14 step 12.
pub async fn write_audit(
    actor: &str,
    action_id: &str,
    target: &str,
    risk_tier: RiskTier,
    payload: Value,
) {
    info!(
        target = "jeryu.web.audit",
        actor,
        action_id,
        audit_target = target,
        risk_tier = risk_tier.label(),
        payload = %payload,
        "audit event"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_tier_labels_match_spec() {
        assert_eq!(RiskTier::Low.label(), "low");
        assert_eq!(RiskTier::Medium.label(), "medium");
        assert_eq!(RiskTier::High.label(), "high");
    }

    #[tokio::test]
    async fn write_audit_does_not_panic() {
        write_audit(
            "@local",
            "repo.create",
            "repo:abc",
            RiskTier::Medium,
            serde_json::json!({"k":"v"}),
        )
        .await;
    }
}
