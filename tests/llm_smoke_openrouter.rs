#![allow(clippy::field_reassign_with_default)]
//! Live smoke test for Phase 2 — Evidence Gate security reviewer end-to-end
//! through OpenRouter.
//!
//! Gated on `JERYU_LLM_LIVE=1`. CI never runs this unsolicited so user budget
//! is not spent without intent. To run locally:
//!
//! ```bash
//! JERYU_LLM_LIVE=1 cargo test --test llm_smoke_openrouter -- --nocapture
//! ```
//!
//! Secrets loaded via the canonical chain, so `OPENROUTER_API_KEY` may come
//! from env, `~/.jeryu/secrets/llm.env`, or repo `.env.local`.

use jeryu::agent_review::{ReviewerCallError, run_security_review, security::SecurityReviewInputs};
use jeryu::autonomy::types::ReviewDecision;
use jeryu::llm::{
    LlmRouter, SecretResolver,
    provider_chains::{build_router_for_roles, load_providers_config},
};

fn live_enabled() -> bool {
    std::env::var("JERYU_LLM_LIVE").as_deref() == Ok("1")
}

const SQL_INJECTION_DIFF: &str = r#"diff --git a/src/api/users.rs b/src/api/users.rs
--- a/src/api/users.rs
+++ b/src/api/users.rs
@@ -38,7 +38,12 @@ pub async fn lookup_by_name(pool: &PgPool, req: LookupReq) -> Result<Vec<User>>
-    let users = sqlx::query_as!(User, "SELECT id, name FROM users WHERE name = $1", req.name)
-        .fetch_all(pool)
-        .await?;
+    // PATCH: speed up by skipping bind parameter
+    let q = format!("SELECT id, name FROM users WHERE name = '{}'", req.name);
+    let users: Vec<User> = sqlx::query_as(&q)
+        .fetch_all(pool)
+        .await?;
     Ok(users)
 }
"#;

fn build_live_router() -> LlmRouter {
    let resolver = SecretResolver::from_env();
    let autonomy = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy");
    let cfg = load_providers_config(&autonomy).expect("canonical providers/llm.yml must load");
    assert!(
        cfg.chains.contains_key("reviewer-security"),
        "canonical providers/llm.yml must declare reviewer-security"
    );
    build_router_for_roles(&autonomy, &["reviewer-security"], &resolver)
        .expect("reviewer-security chain must build from canonical providers/llm.yml")
}

fn load_security_prompt() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".jeryu/autonomy/prompts/reviewer-security.md");
    std::fs::read_to_string(&p).unwrap_or_else(|err| panic!("read {}: {err}", p.display()))
}

#[tokio::test]
#[ignore = "live LLM call; set JERYU_LLM_LIVE=1 to run"]
async fn live_security_review_flags_sql_injection() {
    if !live_enabled() {
        eprintln!("JERYU_LLM_LIVE not set; skipping live test");
        return;
    }
    let router = build_live_router();
    let prompt = load_security_prompt();
    let inputs = SecurityReviewInputs {
        repo: "jeryu/smoke",
        head_sha: &"a".repeat(40),
        policy_sha: &"c".repeat(40),
        target_branch: "main",
        evidence_pack_id: "evp_smoke",
        diff: SQL_INJECTION_DIFF,
        system_prompt_markdown: &prompt,
        evidence_pack_json: None,
    };

    let receipt = match run_security_review(&router, &inputs).await {
        Ok(r) => r,
        Err(e) => panic!("live review failed: {e}"),
    };

    eprintln!("--- live receipt ---");
    eprintln!("{}", serde_json::to_string_pretty(&receipt).unwrap());

    // Either Block (preferred) or Concern is acceptable; Abstain or Pass would
    // mean the reviewer missed an obvious SQLi.
    match receipt.decision {
        ReviewDecision::Block | ReviewDecision::Concern => {}
        other => panic!(
            "expected Block/Concern, got {other:?} ({:?})",
            receipt.reason
        ),
    }
    let mentions_sqli = receipt
        .reason
        .as_deref()
        .map(|s| {
            let l = s.to_ascii_lowercase();
            l.contains("sql") || l.contains("inject")
        })
        .unwrap_or(false)
        || receipt.findings.iter().any(|f| {
            f.class.to_ascii_lowercase().contains("sql")
                || f.class.to_ascii_lowercase().contains("inject")
                || f.evidence
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains("sql")
        });
    assert!(
        mentions_sqli,
        "receipt did not mention SQL/injection: reason={:?} findings={:?}",
        receipt.reason, receipt.findings
    );
    assert!(receipt.prompt_sha.is_some());
    assert!(receipt.raw_response_sha.is_some());
    assert_eq!(receipt.provider.as_deref(), Some("openrouter"));
}

#[tokio::test]
#[ignore = "live LLM call; set JERYU_LLM_LIVE=1 to run"]
async fn live_security_review_passes_clean_diff() {
    if !live_enabled() {
        return;
    }
    let router = build_live_router();
    let prompt = load_security_prompt();
    let clean_diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,5 @@\n /// Greet someone politely.\n+/// Returns a static greeting string.\n pub fn greet() -> &'static str { \"hello, world\" }\n";
    let inputs = SecurityReviewInputs {
        repo: "jeryu/smoke",
        head_sha: &"a".repeat(40),
        policy_sha: &"c".repeat(40),
        target_branch: "main",
        evidence_pack_id: "evp_smoke_clean",
        diff: clean_diff,
        system_prompt_markdown: &prompt,
        evidence_pack_json: None,
    };
    let receipt = run_security_review(&router, &inputs)
        .await
        .expect("clean review");
    eprintln!("clean diff receipt: {:?}", receipt.decision);
    // Pass is correct; Concern (low) is acceptable; Block would be a false positive.
    assert!(matches!(
        receipt.decision,
        ReviewDecision::Pass | ReviewDecision::Concern | ReviewDecision::Abstain
    ));
}

#[tokio::test]
#[ignore = "live LLM call; set JERYU_LLM_LIVE=1 to run"]
async fn live_secret_scrub_aborts_before_calling_llm() {
    if !live_enabled() {
        return;
    }
    let router = build_live_router();
    let prompt = load_security_prompt();
    // Diff embedding a fake AWS-key-shaped fixture — should never reach
    // the LLM. The literal prefix is split so the production secret
    // catalogue does not match this source line at rest; concatenated
    // at runtime the value still trips the live scrubber.
    let aws_key_fixture = concat!("AKI", "AIOSFODNN7EXAMPLE");
    let diff_owned = format!("+ const KEY: &str = \"{}\";\n", aws_key_fixture);
    let diff = diff_owned.as_str();
    let inputs = SecurityReviewInputs {
        repo: "jeryu/smoke",
        head_sha: &"a".repeat(40),
        policy_sha: &"c".repeat(40),
        target_branch: "main",
        evidence_pack_id: "evp_smoke_secret",
        diff,
        system_prompt_markdown: &prompt,
        evidence_pack_json: None,
    };
    unsafe {
        std::env::remove_var("JERYU_LLM_SCRUB_SKIP");
    }
    let err = run_security_review(&router, &inputs)
        .await
        .expect_err("should have failed closed");
    match err {
        ReviewerCallError::SecretScrubFailed { findings } => {
            assert!(findings >= 1);
        }
        other => panic!("expected SecretScrubFailed, got {other:?}"),
    }
}
