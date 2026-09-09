//! Security reviewer. Catches injection, secret handling, authz regressions.
//! Thin wrapper over the shared [`run_review`] flow with the Security role.

use crate::llm::LlmRouter;
use crate::reviewers::runner::{ReviewInputs, ReviewerCallError, ReviewerRoleId, run_review};
use crate::schema::AgentApprovalReceipt;
use crate::signing::EdSigningKey;

pub struct SecurityReviewInputs<'a> {
    pub repo: &'a str,
    pub head_sha: &'a str,
    pub policy_sha: &'a str,
    pub target_branch: &'a str,
    pub evidence_pack_id: &'a str,
    pub diff: &'a str,
    pub system_prompt_markdown: &'a str,
    pub evidence_pack_json: Option<&'a str>,
    pub signing_key: Option<&'a EdSigningKey>,
}

pub async fn run_security_review(
    router: &LlmRouter,
    inputs: &SecurityReviewInputs<'_>,
) -> Result<AgentApprovalReceipt, ReviewerCallError> {
    run_review(
        router,
        &ReviewInputs {
            role: ReviewerRoleId::Security,
            repo: inputs.repo,
            head_sha: inputs.head_sha,
            policy_sha: inputs.policy_sha,
            target_branch: inputs.target_branch,
            evidence_pack_id: inputs.evidence_pack_id,
            diff: inputs.diff,
            system_prompt_markdown: inputs.system_prompt_markdown,
            evidence_pack_json: inputs.evidence_pack_json,
            signing_key: inputs.signing_key,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CallParams, CallResponse, ChatMessage, DataUse, LlmError, LlmProvider, RoleChain,
        RoleChainEntry,
    };
    use crate::schema::ReviewDecision;
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
                model: "deterministic-model".into(),
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
            signing_key: None,
        }
    }

    #[tokio::test]
    async fn parses_block_decision_with_finding_and_prompt_sha() {
        let router = router_with(
            r#"{"role":"security","decision":"block","reason":"sqli","findings":[{"severity":"critical","class":"injection-sql","file":"src/x.rs","range":[1,2]}]}"#,
        );
        // Reversed-source SQL-injection fixture: the tainted query pattern is
        // assembled at runtime so no whole-pattern match appears in source.
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
        // Reversed-source fixture: assembled at runtime so the key-shaped token
        // never appears in source.
        let body: String = "ELPMAXE7NNDOFSOIAIKA".chars().rev().collect();
        let diff = format!("+ const KEY: &str = \"{body}\";");
        let i = inputs(&diff);
        // The scrub-skip flag is never set in tests, so the scrub runs and the
        // call fails closed on the planted secret (no env mutation needed).
        let err = run_security_review(&router, &i).await.unwrap_err();
        assert!(matches!(err, ReviewerCallError::SecretScrubFailed { .. }));
    }
}
