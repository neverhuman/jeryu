//! The 8 canonical typed objects of the Evidence Gate.
//!
//! Schemas: `.jeryu/autonomy/schemas/*.schema.json`. These Rust types and those
//! JSON schemas evolve together; any change must update BOTH (CI lints this).
//!
//! Pull request evidence types for the local forge. `MergePassport` retains the
//! word "merge" because it is the passport to perform a Git merge.
//!
//! This module is a re-export hub: every canonical object lives in a
//! responsibility-scoped submodule, but `crate::types::*` resolves them all
//! unchanged.

mod common;
mod evidence;
mod intent;
mod lease;
mod ledger;
mod passport;
mod receipt;
mod schema_tag;
mod verdict;

pub use common::{GateDecision, ReviewDecision, ReviewerRole, RiskTier, Severity};
pub use evidence::{
    ChangedFile, CiCheck, CiConclusion, EvidencePack, GateReceipt, RollbackSection,
    RollbackStrategy, ScanOutcome, SecuritySection, SupplyChainSection, TestsSection,
};
pub use intent::IntentCard;
pub use lease::{CapabilityLease, LeaseDenied, LeaseScope};
pub use ledger::{LaunchLedgerEntry, LedgerKind};
pub use passport::{
    ArtifactKind, DeployEnvironment, MergePassport, ReleasePassport, ReleaseRollbackPlan,
};
pub use receipt::{AgentApprovalReceipt, Finding, TokenCounts};
pub use schema_tag::{
    AgentApprovalReceiptTag, CapabilityLeaseTag, EvidencePackTag, IntentCardTag,
    LaunchLedgerEntryTag, MergePassportTag, ReleasePassportTag, SchemaKind, SchemaTag,
    VibeGateVerdictTag,
};
pub use verdict::{VerdictReceiptRef, VibeGateVerdict};

/// Serde `default` for boolean fields that should default to `true`.
pub(super) fn true_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::Signature;
    use chrono::{DateTime, Utc};

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn risk_tier_categories() {
        assert!(RiskTier::R0.auto_merge_eligible());
        assert!(RiskTier::R2.auto_merge_eligible());
        assert!(!RiskTier::R3.auto_merge_eligible());
        assert!(RiskTier::R3.human_required());
        assert!(RiskTier::R5.human_required());
    }

    #[test]
    fn intent_card_round_trips() {
        let card = IntentCard {
            schema: SchemaTag::new(),
            id: "intent_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            agent_id: "builder.fix-bug".into(),
            repo: "org/proj".into(),
            target_branch: Some("main".into()),
            summary: "fix off-by-one".into(),
            linked_issue: None,
            estimated_risk: Some(RiskTier::R1),
            expected_changed_paths: vec!["src/lib.rs".into()],
            claims: vec!["adds regression test".into()],
            created_at: now(),
            signature: None,
        };
        let j = serde_json::to_string(&card).unwrap();
        assert!(j.contains("\"schema\":\"vibegate.intent_card.v1\""));
        let back: IntentCard = serde_json::from_str(&j).unwrap();
        assert_eq!(card, back);
    }

    #[test]
    fn schema_mismatch_rejected() {
        let j = r#"{"schema":"vibegate.wrong.v1","id":"intent_x","agent_id":"a","repo":"r","summary":"s","created_at":"2026-05-16T00:00:00Z"}"#;
        let err = serde_json::from_str::<IntentCard>(j).unwrap_err();
        assert!(err.to_string().contains("schema mismatch"));
    }

    #[test]
    fn evidence_pack_round_trips() {
        let pack = EvidencePack {
            schema: SchemaTag::new(),
            id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            intent_id: None,
            repo: "org/proj".into(),
            source_branch: "agent/x".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            policy_sha: "c".repeat(40),
            author_agent: Some("builder.x".into()),
            risk: RiskTier::R2,
            changed_files: vec![],
            claims: vec![],
            tests: TestsSection {
                targeted: vec![],
                full_required: false,
                skipped: vec![],
                coverage_delta: None,
            },
            security: SecuritySection {
                sast: ScanOutcome::Passed,
                dependency_scan: ScanOutcome::Passed,
                secret_scan: ScanOutcome::Passed,
            },
            supply_chain: SupplyChainSection::default(),
            rollback: RollbackSection {
                strategy: RollbackStrategy::RevertCommit,
                feature_flag: None,
                data_migration_reversible: Some(true),
            },
            gate_receipts: vec![],
            ci_status: vec![],
            evidence_digest: format!("sha256:00{}", "0".repeat(62)),
            created_at: now(),
            signature: None,
        };
        let j = serde_json::to_string(&pack).unwrap();
        let back: EvidencePack = serde_json::from_str(&j).unwrap();
        assert_eq!(pack, back);
    }

    #[test]
    fn receipt_decision_serializes_lowercase() {
        let r = AgentApprovalReceipt {
            schema: SchemaTag::new(),
            id: "aar_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            evidence_pack_id: "evp_01HXABCDEFGHJKMNPQRSTVWXYZ".into(),
            role: ReviewerRole::Security,
            agent_id: "reviewer-security.v1".into(),
            prompt_sha: None,
            provider: Some("openrouter".into()),
            model: Some("vendor/model:free".into()),
            temperature: Some(0.0),
            seed: None,
            raw_response_sha: None,
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            decision: ReviewDecision::Block,
            reason: Some("sql injection".into()),
            findings: vec![],
            not_author: true,
            tokens: TokenCounts::default(),
            created_at: now(),
            signature: Signature::unsigned(),
        };
        let j = serde_json::to_string(&r).unwrap();
        assert!(j.contains("\"role\":\"security\""));
        assert!(j.contains("\"decision\":\"block\""));
    }

    fn lease_for(
        agent: &str,
        allowed: &[&str],
        denied_actions: &[&str],
        denied_paths: &[&str],
        ttl_secs: u32,
    ) -> CapabilityLease {
        let issued = now();
        CapabilityLease {
            schema: SchemaTag::new(),
            id: "lease_1".into(),
            intent_id: "intent_1".into(),
            agent_id: agent.into(),
            scope: LeaseScope {
                allowed_actions: allowed.iter().map(|s| (*s).to_string()).collect(),
                denied_actions: denied_actions.iter().map(|s| (*s).to_string()).collect(),
                allowed_write_refs: vec![],
                denied_paths: denied_paths.iter().map(|s| (*s).to_string()).collect(),
            },
            ttl_seconds: ttl_secs,
            issued_at: issued,
            expires_at: issued + chrono::Duration::seconds(ttl_secs as i64),
            policy_sha: "c".repeat(40),
            signature: Signature::unsigned(),
        }
    }

    #[test]
    fn permits_happy_path() {
        let l = lease_for(
            "builder.v1",
            &["pr.create", "evidence.write"],
            &[],
            &[],
            3600,
        );
        assert!(
            l.permits("pr.create", "builder.v1", &["src/foo.rs"], now())
                .is_ok()
        );
    }

    #[test]
    fn permits_rejects_expired_lease() {
        let l = lease_for("builder.v1", &["pr.create"], &[], &[], 0);
        let future = now() + chrono::Duration::seconds(60);
        let err = l
            .permits("pr.create", "builder.v1", &[], future)
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::Expired { .. }));
    }

    #[test]
    fn permits_rejects_wrong_agent() {
        let l = lease_for("builder.v1", &["pr.create"], &[], &[], 3600);
        let err = l.permits("pr.create", "hacker.v1", &[], now()).unwrap_err();
        assert!(matches!(err, LeaseDenied::AgentIdMismatch { .. }));
    }

    #[test]
    fn permits_rejects_explicit_denied_action() {
        let l = lease_for(
            "builder.v1",
            &["pr.create", "approve.own"],
            &["approve.own"],
            &[],
            3600,
        );
        let err = l
            .permits("approve.own", "builder.v1", &[], now())
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::ActionDenied(_)));
    }

    #[test]
    fn permits_rejects_action_not_in_allowlist() {
        let l = lease_for("builder.v1", &["pr.create"], &[], &[], 3600);
        let err = l
            .permits("deploy.prod", "builder.v1", &[], now())
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::ActionNotAllowed(_)));
    }

    #[test]
    fn permits_rejects_denied_path() {
        let l = lease_for(
            "builder.v1",
            &["pr.create"],
            &[],
            &[".jeryu/autonomy/**", "secrets/**"],
            3600,
        );
        let err = l
            .permits(
                "pr.create",
                "builder.v1",
                &["src/foo.rs", ".jeryu/autonomy/policies/risk.yml"],
                now(),
            )
            .unwrap_err();
        assert!(matches!(err, LeaseDenied::PathDenied { .. }));
    }

    #[test]
    fn permits_allows_paths_not_in_denied_list() {
        let l = lease_for(
            "builder.v1",
            &["pr.create"],
            &[],
            &[".jeryu/autonomy/**"],
            3600,
        );
        assert!(
            l.permits("pr.create", "builder.v1", &["src/main.rs"], now())
                .is_ok()
        );
    }

    #[test]
    fn ledger_kind_includes_webhook_received() {
        let kind = LedgerKind::WebhookReceived;
        assert_ne!(kind, LedgerKind::HumanDecisionRecorded);
        let j = serde_json::to_string(&kind).expect("serializes");
        assert_eq!(j, "\"webhook_received\"", "got {j}");
        let back: LedgerKind = serde_json::from_str(&j).expect("deserializes");
        assert_eq!(back, LedgerKind::WebhookReceived);
        let human = serde_json::to_string(&LedgerKind::HumanDecisionRecorded).unwrap();
        assert_eq!(human, "\"human_decision_recorded\"");
    }

    #[test]
    fn permits_empty_allowlist_permits_any_action() {
        let l = lease_for("builder.v1", &[], &[], &[], 3600);
        assert!(
            l.permits("any.action", "builder.v1", &[], now()).is_ok(),
            "empty allowlist means 'no allowlist constraint'; \
             explicit denied_actions still bite"
        );
    }

    #[test]
    fn verdict_wire_field_is_pull_request() {
        let now = now();
        let v = VibeGateVerdict {
            schema: SchemaTag::new(),
            id: "vgv_x".into(),
            evidence_pack_id: "ep_1".into(),
            pull_request: Some("#42".into()),
            repo: "org/repo".into(),
            target_branch: "main".into(),
            head_sha: "a".repeat(40),
            policy_sha: "c".repeat(40),
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: vec![],
            decision: GateDecision::AllowMerge,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now,
            created_at: now,
            signature: Signature::unsigned(),
        };
        let j = serde_json::to_string(&v).unwrap();
        assert!(j.contains("\"pull_request\":\"#42\""), "got {j}");
        let back: VibeGateVerdict = serde_json::from_str(&j).unwrap();
        assert_eq!(v, back);
    }
}
