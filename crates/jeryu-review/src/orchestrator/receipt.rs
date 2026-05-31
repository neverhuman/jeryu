//! Receipt synthesis + signing seam shared by the orchestrators.
//!
//! These helpers build the synthesized `AgentApprovalReceipt`s that the
//! production orchestrator emits for budget short-circuits and failure
//! recovery, and the canned receipts the testing double returns. Every
//! synthesized receipt is SHA-bound to its pack's
//! `(evidence_pack_id, head_sha, policy_sha)` and signed with the
//! orchestrator's ed25519 key so the judge accepts it.

use crate::reviewers::runner::ReviewerRoleId;
use crate::schema::{AgentApprovalReceipt, ReviewDecision, ReviewerRole, SchemaTag, TokenCounts};
use crate::signing::{EdSigningKey, Signature};
use chrono::Utc;

pub(super) fn receipt_role_to_id(role: ReviewerRole) -> ReviewerRoleId {
    match role {
        ReviewerRole::Security => ReviewerRoleId::Security,
        ReviewerRole::TestIntegrity => ReviewerRoleId::TestIntegrity,
        ReviewerRole::Runtime => ReviewerRoleId::Runtime,
        ReviewerRole::Lockfile => ReviewerRoleId::Lockfile,
        ReviewerRole::Nightwatch => ReviewerRoleId::Nightwatch,
        // Judge and ReleaseShepherd have no ReviewerRoleId; the synth_abstain
        // path never reads the prompt for them.
        ReviewerRole::Judge | ReviewerRole::ReleaseShepherd => ReviewerRoleId::Security,
    }
}

pub(crate) fn agent_id_for(role: ReviewerRole) -> &'static str {
    match role {
        ReviewerRole::Security => "reviewer-security.v1",
        ReviewerRole::TestIntegrity => "reviewer-test-integrity.v1",
        ReviewerRole::Runtime => "reviewer-runtime.v1",
        ReviewerRole::Lockfile => "reviewer-lockfile.v1",
        ReviewerRole::Nightwatch => "reviewer-nightwatch.v1",
        ReviewerRole::Judge => "judge.v1",
        ReviewerRole::ReleaseShepherd => "release-shepherd.v1",
    }
}

fn synth_id(role: ReviewerRole, pack_id: &str) -> String {
    let ts = Utc::now().timestamp_millis();
    format!(
        "aar_{role:?}_{pack}_{ts}",
        role = role,
        pack = pack_id.chars().take(12).collect::<String>(),
        ts = ts
    )
}

/// Build an Abstain receipt synthesized by the orchestrator (NOT produced by a
/// per-role reviewer). Used for budget short-circuits and failure recovery.
pub(super) fn synth_abstain(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
    reason: String,
    signing_key: &EdSigningKey,
) -> AgentApprovalReceipt {
    let mut r = AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: None,
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Abstain,
        reason: Some(reason),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    };
    r.signature = sign_canonical(&r, signing_key);
    r
}

/// Sign the canonical JSON projection of `r` (everything except the signature
/// itself, which would be circular).
pub(super) fn sign_canonical(r: &AgentApprovalReceipt, key: &EdSigningKey) -> Signature {
    let mut clone = r.clone();
    clone.signature = Signature::unsigned();
    let body = serde_json::to_string(&clone)
        .expect("AgentApprovalReceipt JSON serialization is infallible");
    key.sign_raw(body.as_bytes())
}

/// Default Pass receipt used by the fake when no canned receipt is registered.
pub(super) fn default_pass_receipt(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: Some("fake".into()),
        model: Some("fake-model".into()),
        temperature: Some(0.0),
        seed: None,
        raw_response_sha: Some(format!("sha256:0{}", "0".repeat(63))),
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Pass,
        reason: Some("fake pass".into()),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}

/// Default Abstain receipt used by the fake's `error_on` path.
pub(super) fn default_abstain_receipt(
    role: ReviewerRole,
    pack_id: &str,
    head_sha: &str,
    policy_sha: &str,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: synth_id(role, pack_id),
        evidence_pack_id: pack_id.to_string(),
        role,
        agent_id: agent_id_for(role).to_string(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: None,
        head_sha: head_sha.to_string(),
        policy_sha: policy_sha.to_string(),
        decision: ReviewDecision::Abstain,
        reason: Some("fake error_on triggered abstain".into()),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}
