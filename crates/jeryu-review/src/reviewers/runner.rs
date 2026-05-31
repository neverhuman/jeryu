//! Generic reviewer dispatch. All reviewer roles share the same flow:
//!   1. Pre-flight: scrub the diff for secrets; fail closed on any finding.
//!   2. Build the (system, user) message pair with prompt-injection defenses.
//!   3. Dispatch through the LLM router for the role's chain key.
//!   4. Parse the strict-schema JSON receipt; emit `abstain` on parse failure
//!      rather than guessing.
//!   5. Sign the receipt (unsigned draft or real ed25519, caller's
//!      choice).
//!
//! Per-role files are thin wrappers that supply a [`ReviewerRoleId`] and
//! delegate here.

use crate::llm::{LlmError, LlmRouter, scrub::scrub_diff};
use crate::parse::extract_receipt_json;
use crate::prompt_builder::{ReviewerPromptInputs, build_reviewer_messages, prompt_sha};
use crate::schema::{
    AgentApprovalReceipt, Finding, ReviewDecision, ReviewerRole, SchemaTag, Severity, TokenCounts,
};
use crate::signing::{EdSigningKey, Signature};
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ReviewerCallError {
    #[error("pre-flight secret scrub failed: {findings} finding(s); aborting LLM call")]
    SecretScrubFailed { findings: usize },
    #[error("LLM provider error: {0}")]
    Provider(#[from] LlmError),
}

/// What kind of reviewer to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewerRoleId {
    Security,
    TestIntegrity,
    Runtime,
    Lockfile,
    Nightwatch,
}

impl ReviewerRoleId {
    /// Per-role chain key — preserved verbatim so prompt replay binds.
    pub fn chain_key(self) -> &'static str {
        match self {
            ReviewerRoleId::Security => "reviewer-security",
            ReviewerRoleId::TestIntegrity => "reviewer-test-integrity",
            ReviewerRoleId::Runtime => "reviewer-runtime",
            ReviewerRoleId::Lockfile => "reviewer-lockfile",
            ReviewerRoleId::Nightwatch => "reviewer-nightwatch",
        }
    }
    pub fn agent_id(self) -> &'static str {
        match self {
            ReviewerRoleId::Security => "reviewer-security.v1",
            ReviewerRoleId::TestIntegrity => "reviewer-test-integrity.v1",
            ReviewerRoleId::Runtime => "reviewer-runtime.v1",
            ReviewerRoleId::Lockfile => "reviewer-lockfile.v1",
            ReviewerRoleId::Nightwatch => "reviewer-nightwatch.v1",
        }
    }
    pub fn to_receipt_role(self) -> ReviewerRole {
        match self {
            ReviewerRoleId::Security => ReviewerRole::Security,
            ReviewerRoleId::TestIntegrity => ReviewerRole::TestIntegrity,
            ReviewerRoleId::Runtime => ReviewerRole::Runtime,
            ReviewerRoleId::Lockfile => ReviewerRole::Lockfile,
            ReviewerRoleId::Nightwatch => ReviewerRole::Nightwatch,
        }
    }
    pub fn prompt_path(self) -> &'static str {
        match self {
            ReviewerRoleId::Security => "prompts/reviewer-security.md",
            ReviewerRoleId::TestIntegrity => "prompts/reviewer-test-integrity.md",
            ReviewerRoleId::Runtime => "prompts/reviewer-runtime.md",
            ReviewerRoleId::Lockfile => "prompts/lockfile-scout.md",
            ReviewerRoleId::Nightwatch => "prompts/reviewer-nightwatch.md",
        }
    }
}

pub struct ReviewInputs<'a> {
    pub role: ReviewerRoleId,
    pub repo: &'a str,
    pub head_sha: &'a str,
    pub policy_sha: &'a str,
    pub target_branch: &'a str,
    pub evidence_pack_id: &'a str,
    pub diff: &'a str,
    pub system_prompt_markdown: &'a str,
    pub evidence_pack_json: Option<&'a str>,
    /// Optional real signing key. If `None`, receipts carry
    /// `Signature::unsigned()` and will be rejected by the judge's
    /// `evidence_signature_invalid` condition.
    pub signing_key: Option<&'a EdSigningKey>,
}

pub async fn run_review(
    router: &LlmRouter,
    inputs: &ReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    // 1. Pre-flight scrub (fail-closed).
    let scrub = scrub_diff(inputs.diff);
    if !scrub.passed {
        return Err(ReviewerCallError::SecretScrubFailed {
            findings: scrub.findings.len(),
        });
    }
    // 2. Build messages.
    let messages = build_reviewer_messages(&ReviewerPromptInputs {
        system_prompt_markdown: inputs.system_prompt_markdown,
        repo: inputs.repo,
        head_sha: inputs.head_sha,
        target_branch: inputs.target_branch,
        diff: inputs.diff,
        evidence_pack_json: inputs.evidence_pack_json,
    });
    let prompt_hash = prompt_sha(inputs.system_prompt_markdown);
    // 3. Dispatch via router.
    let resp = router.dispatch(inputs.role.chain_key(), &messages).await?;
    // 4. Parse the receipt (abstain on parse failure).
    let now = Utc::now();
    let id = format!("aar_{}", ulid_like(&resp.raw_response_sha, now));
    let mut receipt = match extract_receipt_json(&resp.content) {
        Ok(p) => map_parsed_to_receipt(p, &resp, inputs, &id, prompt_hash, now),
        Err(parse_err) => abstain_receipt(
            inputs,
            &id,
            prompt_hash,
            &resp,
            format!("response did not parse: {parse_err}"),
            now,
        ),
    };
    // 5. Sign with real ed25519 if a key is supplied.
    if let Some(key) = inputs.signing_key {
        receipt.signature = sign_receipt(&receipt, key);
    }
    Ok(receipt)
}

/// Sign the canonical JSON projection of `r` (everything except the signature
/// itself, which would be circular).
pub fn sign_receipt(r: &AgentApprovalReceipt, key: &EdSigningKey) -> Signature {
    let mut clone = r.clone();
    clone.signature = Signature::unsigned();
    let body = serde_json::to_string(&clone)
        .expect("AgentApprovalReceipt JSON serialization is infallible");
    key.sign_raw(body.as_bytes())
}

fn map_parsed_to_receipt(
    p: crate::parse::ParsedReceiptFields,
    resp: &crate::llm::CallResponse,
    inputs: &ReviewInputs<'_>,
    id: &str,
    prompt_hash: String,
    now: chrono::DateTime<Utc>,
) -> AgentApprovalReceipt {
    let decision = match p.decision.to_ascii_lowercase().as_str() {
        "pass" => ReviewDecision::Pass,
        "concern" => ReviewDecision::Concern,
        "block" => ReviewDecision::Block,
        _ => ReviewDecision::Abstain,
    };
    let findings = p.findings.into_iter().filter_map(parse_finding).collect();
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: id.to_string(),
        evidence_pack_id: inputs.evidence_pack_id.to_string(),
        role: inputs.role.to_receipt_role(),
        agent_id: inputs.role.agent_id().to_string(),
        prompt_sha: Some(prompt_hash),
        provider: Some(resp.provider.clone()),
        model: Some(resp.model.clone()),
        temperature: Some(0.0),
        seed: None,
        raw_response_sha: Some(resp.raw_response_sha.clone()),
        head_sha: inputs.head_sha.to_string(),
        policy_sha: inputs.policy_sha.to_string(),
        decision,
        reason: p.reason,
        findings,
        not_author: true,
        tokens: TokenCounts {
            prompt: resp.prompt_tokens.unwrap_or(0),
            completion: resp.completion_tokens.unwrap_or(0),
        },
        created_at: now,
        signature: Signature::unsigned(),
    }
}

fn abstain_receipt(
    inputs: &ReviewInputs<'_>,
    id: &str,
    prompt_hash: String,
    resp: &crate::llm::CallResponse,
    reason: String,
    now: chrono::DateTime<Utc>,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: id.to_string(),
        evidence_pack_id: inputs.evidence_pack_id.to_string(),
        role: inputs.role.to_receipt_role(),
        agent_id: inputs.role.agent_id().to_string(),
        prompt_sha: Some(prompt_hash),
        provider: Some(resp.provider.clone()),
        model: Some(resp.model.clone()),
        temperature: Some(0.0),
        seed: None,
        raw_response_sha: Some(resp.raw_response_sha.clone()),
        head_sha: inputs.head_sha.to_string(),
        policy_sha: inputs.policy_sha.to_string(),
        decision: ReviewDecision::Abstain,
        reason: Some(reason),
        findings: vec![],
        not_author: true,
        tokens: TokenCounts {
            prompt: resp.prompt_tokens.unwrap_or(0),
            completion: resp.completion_tokens.unwrap_or(0),
        },
        created_at: now,
        signature: Signature::unsigned(),
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ParsedFinding {
    severity: Option<String>,
    class: Option<String>,
    file: Option<String>,
    range: Option<[u32; 2]>,
    #[serde(default)]
    evidence: Option<String>,
    #[serde(default)]
    recommendation: Option<String>,
}

fn parse_finding(v: serde_json::Value) -> Option<Finding> {
    let pf: ParsedFinding = serde_json::from_value(v).ok()?;
    Some(Finding {
        severity: match pf.severity?.to_ascii_lowercase().as_str() {
            "info" => Severity::Info,
            "low" => Severity::Low,
            "medium" => Severity::Medium,
            "high" => Severity::High,
            "critical" => Severity::Critical,
            _ => return None,
        },
        class: pf.class?,
        file: pf.file?,
        range: pf.range?,
        evidence: pf.evidence,
        recommendation: pf.recommendation,
    })
}

/// Cheap ULID-shape id (NOT a real ULID; collision-free enough for local use).
fn ulid_like(seed_hex: &str, now: chrono::DateTime<Utc>) -> String {
    let mut s = String::with_capacity(26);
    let ts = now.timestamp_millis() as u64;
    s.push_str(&format!("{ts:013X}"));
    let tail: String = seed_hex
        .chars()
        .rev()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(13)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    s.push_str(&tail);
    while s.len() < 26 {
        s.push('0');
    }
    s.truncate(26);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ids_map_to_distinct_chain_keys() {
        let r = [
            ReviewerRoleId::Security,
            ReviewerRoleId::TestIntegrity,
            ReviewerRoleId::Runtime,
            ReviewerRoleId::Lockfile,
            ReviewerRoleId::Nightwatch,
        ];
        let mut keys: Vec<&str> = r.iter().map(|r| r.chain_key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 5, "every role must have a unique chain key");
    }

    #[test]
    fn role_to_receipt_role_round_trips() {
        assert_eq!(
            ReviewerRoleId::Security.to_receipt_role(),
            ReviewerRole::Security
        );
        assert_eq!(
            ReviewerRoleId::Nightwatch.to_receipt_role(),
            ReviewerRole::Nightwatch
        );
    }
}
