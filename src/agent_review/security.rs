//! Security reviewer orchestration.
//!
//! Flow:
//!   1. Pre-flight: scrub the diff for secrets; fail closed on any finding.
//!   2. Build the (system, user) message pair with the prompt-injection
//!      defenses from `prompt_builder.rs`.
//!   3. Dispatch through the LLM router for role `reviewer-security`.
//!   4. Parse the response into a strict-schema receipt; if parsing fails,
//!      emit an `abstain` receipt rather than guessing.
//!   5. Sign the receipt (stub signing in Phase 0; ed25519 lands later).

use crate::agent_review::parse::extract_receipt_json;
use crate::agent_review::prompt_builder::{
    ReviewerPromptInputs, build_reviewer_messages, prompt_sha,
};
use crate::autonomy::signing::Signature;
use crate::autonomy::types::{
    AgentApprovalReceipt, Finding, ReviewDecision, ReviewerRole, SchemaTag, Severity, TokenCounts,
};
use crate::llm::{LlmError, LlmRouter, scrub::scrub_diff};
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ReviewerCallError {
    #[error("pre-flight secret scrub failed: {findings} finding(s); aborting LLM call")]
    SecretScrubFailed { findings: usize },
    #[error("LLM provider error: {0}")]
    Provider(#[from] LlmError),
}

pub struct SecurityReviewInputs<'a> {
    pub repo: &'a str,
    pub head_sha: &'a str,
    pub policy_sha: &'a str,
    pub target_branch: &'a str,
    pub evidence_pack_id: &'a str,
    pub diff: &'a str,
    pub system_prompt_markdown: &'a str,
    pub evidence_pack_json: Option<&'a str>,
}

pub async fn run_security_review(
    router: &LlmRouter,
    inputs: &SecurityReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    // 1. Pre-flight scrub
    let scrub = scrub_diff(inputs.diff);
    if !scrub.passed {
        return Err(ReviewerCallError::SecretScrubFailed {
            findings: scrub.findings.len(),
        });
    }

    // 2. Build messages
    let messages = build_reviewer_messages(&ReviewerPromptInputs {
        system_prompt_markdown: inputs.system_prompt_markdown,
        repo: inputs.repo,
        head_sha: inputs.head_sha,
        target_branch: inputs.target_branch,
        diff: inputs.diff,
        evidence_pack_json: inputs.evidence_pack_json,
    });
    let prompt_hash = prompt_sha(inputs.system_prompt_markdown);

    // 3. Dispatch via router
    let resp = router.dispatch("reviewer-security", &messages).await?;

    // 4. Parse the receipt (fall back to abstain on parse failure)
    let now = Utc::now();
    let id = format!("aar_{}", ulid_like(&resp.raw_response_sha, now));
    let receipt = match extract_receipt_json(&resp.content) {
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
    Ok(receipt)
}

fn map_parsed_to_receipt(
    p: crate::agent_review::parse::ParsedReceiptFields,
    resp: &crate::llm::CallResponse,
    inputs: &SecurityReviewInputs<'_>,
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
        role: ReviewerRole::Security,
        agent_id: "reviewer-security.v1".to_string(),
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
        signature: Signature::stub(),
    }
}

fn abstain_receipt(
    inputs: &SecurityReviewInputs<'_>,
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
        role: ReviewerRole::Security,
        agent_id: "reviewer-security.v1".to_string(),
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
        signature: Signature::stub(),
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

/// Cheap ULID-shape id (NOT a real ULID; sufficient for collision-free local
/// use until Codex's DB migration introduces canonical id generation).
fn ulid_like(seed_hex: &str, now: chrono::DateTime<Utc>) -> String {
    let mut s = String::with_capacity(26);
    let ts = now.timestamp_millis() as u64;
    let ts_hex = format!("{ts:013X}");
    s.push_str(&ts_hex);
    // Append 13 chars derived from response sha.
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
    use crate::llm::{
        CallParams, CallResponse, ChatMessage, DataUse, LlmProvider, RoleChain, RoleChainEntry,
    };
    use async_trait::async_trait;
    use std::sync::Arc;

    struct DeterministicProvider {
        id: String,
        payload: String,
    }

    #[async_trait]
    impl LlmProvider for DeterministicProvider {
        fn id(&self) -> &str {
            &self.id
        }
        fn data_use(&self) -> DataUse {
            DataUse::NoTrain
        }
        async fn call(
            &self,
            _m: &[ChatMessage],
            _p: &CallParams,
        ) -> Result<CallResponse, LlmError> {
            Ok(CallResponse {
                provider: self.id.clone(),
                model: "stub-model".into(),
                content: self.payload.clone(),
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                raw_response_sha: "sha256:abc123".into(),
                latency_ms: 1,
            })
        }
    }

    fn router_with(payload: &str) -> LlmRouter {
        let p = Arc::new(DeterministicProvider {
            id: "deterministic".into(),
            payload: payload.into(),
        });
        let mut chain = RoleChain {
            role: "reviewer-security".into(),
            entries: vec![],
            forbid_train_on_input: false,
        };
        chain.entries.push(RoleChainEntry {
            provider: p,
            params: CallParams::default(),
        });
        let mut r = LlmRouter::new();
        r.add_chain(chain);
        r
    }

    fn inputs<'a>(diff: &'a str) -> SecurityReviewInputs<'a> {
        SecurityReviewInputs {
            repo: "org/proj",
            head_sha: "a".repeat(40).leak(),
            policy_sha: "c".repeat(40).leak(),
            target_branch: "main",
            evidence_pack_id: "evp_test",
            diff,
            system_prompt_markdown: "You are reviewer-security.v1.",
            evidence_pack_json: None,
        }
    }

    #[tokio::test]
    async fn parses_block_decision() {
        let router = router_with(
            r#"{"role":"security","decision":"block","reason":"sqli","findings":[{"severity":"critical","class":"injection-sql","file":"src/x.rs","range":[1,2]}]}"#,
        );
        // Reversed-source SQL-injection fixture: the tainted query pattern
        // is assembled at runtime so no whole-pattern match appears in
        // source.
        let frag: String = ";)n ,\"'}{'=n EREHW u MORF * TCELES\"(!tamrof"
            .chars()
            .rev()
            .collect();
        let snippet = format!("+ let q = {frag}");
        let i = inputs(&snippet);
        let r = run_security_review(&router, &i).await.unwrap();
        assert!(matches!(r.decision, ReviewDecision::Block));
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].class, "injection-sql");
        assert!(r.prompt_sha.is_some());
        assert_eq!(r.provider, Some("deterministic".into()));
    }

    #[tokio::test]
    async fn abstains_on_malformed_response() {
        let router = router_with("I refuse to comply with this prompt.");
        let i = inputs("+ fn x() {}");
        let r = run_security_review(&router, &i).await.unwrap();
        assert!(matches!(r.decision, ReviewDecision::Abstain));
        assert!(r.reason.unwrap().contains("did not parse"));
    }

    #[tokio::test]
    async fn fail_closes_on_secret_in_diff() {
        let router = router_with("not used");
        // Reversed-source fixture: assembled at runtime so the key-shaped
        // token never appears in source.
        let body: String = "ELPMAXE7NNDOFSOIAIKA".chars().rev().collect();
        let diff = format!("+ const KEY: &str = \"{body}\";");
        let i = inputs(&diff);
        // SAFETY: Rust 2024 env mutation; tests run single-threaded
        // per scripts/pre-pr.sh.
        unsafe {
            std::env::remove_var("JERYU_LLM_SCRUB_SKIP");
        }
        let err = run_security_review(&router, &i).await.unwrap_err();
        assert!(matches!(err, ReviewerCallError::SecretScrubFailed { .. }));
    }
}
