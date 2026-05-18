//! Live GitHub adapter test (read-only).
//!
//! Uses `GITHUB_TOKEN` from the standard secrets chain. Only calls
//! `ping_user` — never writes — so it is safe to run against the user's
//! actual token.
//!
//! Gated on `JERYU_LLM_LIVE=1` (same gate as LLM live tests).

use jeryu::git_host::{GitHost, GitHubClient};
use jeryu::llm::{SecretResolver, resolve_secret};

#[tokio::test]
#[ignore = "live GitHub API call; set JERYU_LLM_LIVE=1 to run"]
async fn ping_user_returns_login() {
    if std::env::var("JERYU_LLM_LIVE").as_deref() != Ok("1") {
        eprintln!("JERYU_LLM_LIVE not set; skipping");
        return;
    }
    let resolver = SecretResolver::from_env();
    let token = match resolve_secret("GITHUB_TOKEN", &resolver) {
        Some(s) => s.value,
        None => {
            eprintln!("GITHUB_TOKEN not in secrets chain; skipping");
            return;
        }
    };
    let client = GitHubClient::new(token);
    let identity = client.ping_user().await.expect("ping_user");
    eprintln!("[live] github user: {}", identity.login);
    assert_eq!(identity.host, "github");
    assert!(!identity.login.is_empty(), "login must be non-empty");
}

#[tokio::test]
#[ignore = "live GitHub API call; set JERYU_LLM_LIVE=1 to run"]
async fn approve_mr_dry_run_path_works_live() {
    if std::env::var("JERYU_LLM_LIVE").as_deref() != Ok("1") {
        return;
    }
    let resolver = SecretResolver::from_env();
    let token = match resolve_secret("GITHUB_TOKEN", &resolver) {
        Some(s) => s.value,
        None => return,
    };
    let client = GitHubClient::new(token);
    let repo = jeryu::git_host::RepoRef::parse("jeryu/dummy").unwrap();
    let r = client
        .approve_mr(jeryu::git_host::MrApproval {
            repo: &repo,
            mr_iid: "1",
            head_sha: &"a".repeat(40),
            agent_id: "reviewer-security.v1",
            receipt_digest: "sha256:beef",
            dry_run: true,
        })
        .await
        .expect("dry-run approve");
    // Dry run never hits the network; result should begin with "dry-run".
    assert!(r.starts_with("dry-run"), "expected dry-run prefix, got {r}");
}
