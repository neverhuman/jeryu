//! Named hard-stop condition registry.
//!
//! Policy `hard_stops:` reference vetted *names* defined here. Unknown names
//! fail closed to an `unknown_condition:<name>` hard stop. No runtime
//! string-eval, no expression parser — adding a condition is a code change.
//!
//! This crate ships the deterministic conditions the judge fuses over. Richer
//! conditions that need context beyond `(pack, receipts)` (codeowners, freeze
//! windows, budget) are registered as `externally_supplied` — they return
//! `None` here and the orchestrator injects them via `external_hard_stops`.

use crate::schema::{AgentApprovalReceipt, EvidencePack, ReviewDecision, ScanOutcome};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardStop {
    pub name: String,
    pub reason: String,
    #[serde(default)]
    pub details: serde_json::Value,
}

/// Signature of a named condition.
pub type CondFn = fn(&EvidencePack, &[AgentApprovalReceipt]) -> Option<HardStop>;

#[derive(Debug, Clone, Copy)]
pub struct NamedCondition {
    pub name: &'static str,
    pub func: CondFn,
}

pub struct ConditionRegistry {
    table: Vec<NamedCondition>,
}

impl Default for ConditionRegistry {
    fn default() -> Self {
        let table = vec![
            NamedCondition {
                name: "evidence_missing",
                func: cond_evidence_missing,
            },
            NamedCondition {
                name: "evidence_signature_invalid",
                func: cond_evidence_signature_invalid,
            },
            NamedCondition {
                name: "secret_scan_failed",
                func: cond_secret_scan_failed,
            },
            NamedCondition {
                name: "secret_scan_missing",
                func: cond_secret_scan_missing,
            },
            NamedCondition {
                name: "sast_failed",
                func: cond_sast_failed,
            },
            NamedCondition {
                name: "dependency_scan_failed",
                func: cond_dependency_scan_failed,
            },
            NamedCondition {
                name: "reviewer_blocked",
                func: cond_reviewer_blocked,
            },
            NamedCondition {
                name: "reviewer_abstained_required",
                func: cond_reviewer_abstained_required,
            },
            NamedCondition {
                name: "lockfile_only_change",
                func: cond_lockfile_only_change,
            },
            // Externally supplied: judge/orchestrator injects via
            // external_hard_stops; the registry keeps them total so policy
            // references never fail closed for a known-but-external name.
            NamedCondition {
                name: "codeowners_not_satisfied",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "freeze_window_active",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "budget_exceeded",
                func: cond_externally_supplied,
            },
            NamedCondition {
                name: "sha_drift",
                func: cond_externally_supplied,
            },
        ];
        Self { table }
    }
}

impl ConditionRegistry {
    pub fn lookup(&self, name: &str) -> Option<NamedCondition> {
        self.table.iter().copied().find(|c| c.name == name)
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.table.iter().map(|c| c.name).collect()
    }

    /// Evaluate every requested named condition; returns triggered hard stops in
    /// registry order. Unknown names become an `unknown_condition:<name>`
    /// hard-stop (fail-closed).
    pub fn evaluate(
        &self,
        requested: &[String],
        pack: &EvidencePack,
        receipts: &[AgentApprovalReceipt],
    ) -> Vec<HardStop> {
        let mut out = Vec::new();
        for name in requested {
            match self.lookup(name) {
                Some(c) => {
                    if let Some(h) = (c.func)(pack, receipts) {
                        out.push(h);
                    }
                }
                None => out.push(HardStop {
                    name: format!("unknown_condition:{name}"),
                    reason: "policy references a condition not in the registry; fail-closed".into(),
                    details: serde_json::Value::Null,
                }),
            }
        }
        out
    }
}

// --- named conditions ------------------------------------------------------

fn cond_evidence_missing(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    p.evidence_digest.is_empty().then(|| HardStop {
        name: "evidence_missing".into(),
        reason: "evidence_pack has no digest".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_evidence_signature_invalid(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    match &p.signature {
        // Real ed25519 is accepted (the verifier path cross-checks elsewhere).
        Some(s) if s.algo == "ed25519" => None,
        Some(s) if s.algo == "unsigned" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack is unsigned (algo: 'unsigned'); not acceptable in enforcement"
                .into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) if s.algo == "hmac-insecure" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack signed with insecure HMAC; ed25519 required in enforcement"
                .into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: format!("evidence pack signed with unknown algo '{}'", s.algo),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        None => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack is unsigned".into(),
            details: serde_json::Value::Null,
        }),
    }
}

fn cond_secret_scan_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "secret_scan_failed".into(),
        reason: "secret scan reported findings".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_secret_scan_missing(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Missing).then(|| HardStop {
        name: "secret_scan_missing".into(),
        reason: "secret scan never ran; fail-closed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_sast_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.sast, ScanOutcome::Failed).then(|| HardStop {
        name: "sast_failed".into(),
        reason: "SAST scan failed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_dependency_scan_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.dependency_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "dependency_scan_failed".into(),
        reason: "dependency scan failed".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_reviewer_blocked(_p: &EvidencePack, receipts: &[AgentApprovalReceipt]) -> Option<HardStop> {
    let blockers: Vec<&AgentApprovalReceipt> = receipts
        .iter()
        .filter(|r| matches!(r.decision, ReviewDecision::Block))
        .collect();
    if blockers.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "reviewer_blocked".into(),
        reason: format!("{} reviewer(s) issued a hard block", blockers.len()),
        details: serde_json::json!({
            "roles": blockers.iter().map(|r| r.role).collect::<Vec<_>>(),
            "agents": blockers.iter().map(|r| r.agent_id.clone()).collect::<Vec<_>>(),
        }),
    })
}

fn cond_reviewer_abstained_required(
    _p: &EvidencePack,
    receipts: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let any = receipts
        .iter()
        .any(|r| matches!(r.decision, ReviewDecision::Abstain));
    any.then(|| HardStop {
        name: "reviewer_abstained_required".into(),
        reason: "a required reviewer abstained".into(),
        details: serde_json::Value::Null,
    })
}

fn cond_lockfile_only_change(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    p.supply_chain.lockfile_only_change.then(|| HardStop {
        name: "lockfile_only_change".into(),
        reason: "change touches only the lockfile".into(),
        details: serde_json::Value::Null,
    })
}

/// Stand-in for conditions whose context lives outside `(pack, receipts)`.
/// Always `None`; the orchestrator injects the real hit via external hard stops.
fn cond_externally_supplied(_p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ReviewerRole;
    use crate::test_support::{pack_with, receipt_for};

    #[test]
    fn unknown_condition_fails_closed() {
        let reg = ConditionRegistry::default();
        let p = pack_with(crate::schema::RiskTier::R2, true, ScanOutcome::Passed);
        let hits = reg.evaluate(&["totally_made_up".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].name.starts_with("unknown_condition:"));
    }

    #[test]
    fn reviewer_blocked_triggers() {
        let reg = ConditionRegistry::default();
        let p = pack_with(crate::schema::RiskTier::R2, true, ScanOutcome::Passed);
        let blocked = receipt_for(ReviewerRole::Security, "sec.v1", ReviewDecision::Block, &p);
        let hits = reg.evaluate(&["reviewer_blocked".into()], &p, &[blocked]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "reviewer_blocked");
    }

    #[test]
    fn secret_scan_failed_triggers() {
        let reg = ConditionRegistry::default();
        let p = pack_with(crate::schema::RiskTier::R2, true, ScanOutcome::Failed);
        let hits = reg.evaluate(&["secret_scan_failed".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "secret_scan_failed");
    }

    #[test]
    fn unsigned_pack_triggers_signature_invalid() {
        let reg = ConditionRegistry::default();
        let p = pack_with(crate::schema::RiskTier::R2, false, ScanOutcome::Passed);
        let hits = reg.evaluate(&["evidence_signature_invalid".into()], &p, &[]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "evidence_signature_invalid");
    }

    #[test]
    fn externally_supplied_returns_none_in_registry() {
        let reg = ConditionRegistry::default();
        let p = pack_with(crate::schema::RiskTier::R2, true, ScanOutcome::Passed);
        let hits = reg.evaluate(&["codeowners_not_satisfied".into()], &p, &[]);
        assert!(hits.is_empty(), "external condition does not self-trigger");
    }
}
