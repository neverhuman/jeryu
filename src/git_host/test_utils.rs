//! In-memory `GitHost` fake for unit tests.
//!
//! Exposed publicly (not `#[cfg(test)]`-gated) so the Wave 7.C autonomy
//! daemon — which lives in a sibling module — can use it from its own
//! test suite. The cost of leaking this into `pub` is negligible (a few
//! `Arc<Mutex<…>>` fields) and avoids the alternative of either a
//! `test-utils` cargo feature (extra build matrix) or duplicating the
//! same fixture in every consumer.
//!
//! All trait methods record every call into `recorded_calls` so tests
//! can assert what the daemon (or any caller) actually invoked. Use
//! `fail_next` to force the next call to a named method to return
//! `HostError::Transient` — handy for exercising retry / backoff paths.
//!
//! Recording happens BEFORE the seeded lookup so a caller can still
//! assert the call landed even when the method returns an error.

use crate::git_host::{
    CheckRun, CheckRunResult, CheckStatus, GitHost, HostError, HostIdentity, MrApproval, PrDiff,
    PrLiveState, PrSummary, RepoRef,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Every trait method records into this enum so tests can assert what
/// the system-under-test actually called. Fields are owned (not borrowed)
/// so the recorded call survives past the borrow lifetimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCall {
    PingUser,
    ListOpenPrs {
        repo: String,
    },
    GetPrState {
        repo: String,
        mr_iid: String,
    },
    FetchPrDiff {
        repo: String,
        mr_iid: String,
    },
    PostCheckRun {
        repo: String,
        head_sha: String,
        name: String,
        status: CheckStatus,
    },
    PostMrComment {
        repo: String,
        mr_iid: String,
        body: String,
    },
    ApproveMr {
        repo: String,
        mr_iid: String,
        head_sha: String,
        agent_id: String,
        dry_run: bool,
    },
    /// Wave 7.B: `GitHost::fetch_target_policy_sha` was invoked. Recorded
    /// even when the seeded value is `None` so tests can prove the
    /// auto-rejudge code reached for the host (rather than skipping the
    /// call entirely).
    FetchTargetPolicySha {
        repo: String,
        target_branch: String,
    },
}

pub struct FakeGitHost {
    pub identity: HostIdentity,
    pub open_prs: Arc<Mutex<HashMap<String, Vec<PrSummary>>>>,
    pub pr_states: Arc<Mutex<HashMap<(String, String), PrLiveState>>>,
    pub pr_diffs: Arc<Mutex<HashMap<(String, String), PrDiff>>>,
    /// Wave 7.B: seeded `fetch_target_policy_sha` return values, keyed by
    /// `(repo_slug, target_branch)`. Stored as `Option<String>` so tests
    /// can distinguish "we seeded an explicit None" from "we never
    /// seeded anything" (the latter falls through to `Ok(None)` per the
    /// trait default).
    #[allow(clippy::type_complexity)]
    pub target_policy_shas: Arc<Mutex<HashMap<(String, String), Option<String>>>>,
    pub recorded_calls: Arc<Mutex<Vec<RecordedCall>>>,
    /// When set, the NEXT call to a method whose name matches this string
    /// returns `HostError::Transient` and clears the flag. Method names
    /// match the trait method (e.g. "ping_user", "list_open_prs",
    /// "get_pr_state", "fetch_pr_diff", "post_check_run", "post_mr_comment",
    /// "approve_mr", "fetch_target_policy_sha").
    pub fail_on: Arc<Mutex<Option<String>>>,
}

impl Default for FakeGitHost {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeGitHost {
    pub fn new() -> Self {
        Self {
            identity: HostIdentity {
                login: "fake-bot".into(),
                host: "fake".into(),
            },
            open_prs: Arc::new(Mutex::new(HashMap::new())),
            pr_states: Arc::new(Mutex::new(HashMap::new())),
            pr_diffs: Arc::new(Mutex::new(HashMap::new())),
            target_policy_shas: Arc::new(Mutex::new(HashMap::new())),
            recorded_calls: Arc::new(Mutex::new(Vec::new())),
            fail_on: Arc::new(Mutex::new(None)),
        }
    }

    /// Seed the open-PR list for a repo slug (e.g. "owner/name").
    pub fn with_open_prs(self, repo: &str, prs: Vec<PrSummary>) -> Self {
        self.open_prs.lock().unwrap().insert(repo.to_string(), prs);
        self
    }

    /// Seed the live state for a single PR.
    pub fn with_pr_state(self, repo: &str, mr_iid: &str, state: PrLiveState) -> Self {
        self.pr_states
            .lock()
            .unwrap()
            .insert((repo.to_string(), mr_iid.to_string()), state);
        self
    }

    /// Seed the diff for a single PR. Used by the Wave 8 `EvidencePackBuilder`
    /// tests so they don't have to round-trip through a real `GitHost`.
    pub fn with_pr_diff(self, repo: &str, mr_iid: &str, diff: PrDiff) -> Self {
        self.pr_diffs
            .lock()
            .unwrap()
            .insert((repo.to_string(), mr_iid.to_string()), diff);
        self
    }

    /// Wave 7.B: seed the `fetch_target_policy_sha` return value for a
    /// `(repo, target_branch)` pair. Pass `Some(sha)` to model "the
    /// target branch has `.jeryu/autonomy/policies` and hashes to this SHA",
    /// or `None` to model "host reports no policy on the target branch"
    /// (the trait default behavior, but explicit here so tests can
    /// assert the daemon still recorded the call).
    pub fn with_target_policy_sha(
        self,
        repo: &str,
        target_branch: &str,
        sha: Option<String>,
    ) -> Self {
        self.target_policy_shas
            .lock()
            .unwrap()
            .insert((repo.to_string(), target_branch.to_string()), sha);
        self
    }

    /// Force the NEXT call to a method with this name to return
    /// `HostError::Transient`. Cleared after firing once.
    pub fn fail_next(self, method: &str) -> Self {
        *self.fail_on.lock().unwrap() = Some(method.to_string());
        self
    }

    fn record(&self, call: RecordedCall) {
        self.recorded_calls.lock().unwrap().push(call);
    }

    /// If `fail_on` matches `method`, clear it and return `true`.
    fn should_fail(&self, method: &str) -> bool {
        let mut guard = self.fail_on.lock().unwrap();
        if guard.as_deref() == Some(method) {
            *guard = None;
            true
        } else {
            false
        }
    }

    /// Convenience for tests: snapshot of recorded calls.
    pub fn calls(&self) -> Vec<RecordedCall> {
        self.recorded_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl GitHost for FakeGitHost {
    fn id(&self) -> &str {
        "fake"
    }

    async fn ping_user(&self) -> Result<HostIdentity, HostError> {
        self.record(RecordedCall::PingUser);
        if self.should_fail("ping_user") {
            return Err(HostError::Transient("forced fail: ping_user".into()));
        }
        Ok(self.identity.clone())
    }

    async fn post_check_run(&self, input: CheckRun<'_>) -> Result<CheckRunResult, HostError> {
        self.record(RecordedCall::PostCheckRun {
            repo: input.repo.slug(),
            head_sha: input.head_sha.to_string(),
            name: input.name.to_string(),
            status: input.status,
        });
        if self.should_fail("post_check_run") {
            return Err(HostError::Transient("forced fail: post_check_run".into()));
        }
        Ok(CheckRunResult {
            id: format!("fake-check-{}", input.head_sha),
            url: None,
        })
    }

    async fn post_mr_comment(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, HostError> {
        self.record(RecordedCall::PostMrComment {
            repo: repo.slug(),
            mr_iid: mr_iid.to_string(),
            body: body.to_string(),
        });
        if self.should_fail("post_mr_comment") {
            return Err(HostError::Transient("forced fail: post_mr_comment".into()));
        }
        Ok(format!("fake-comment-{}-{}", repo.slug(), mr_iid))
    }

    async fn approve_mr(&self, input: MrApproval<'_>) -> Result<String, HostError> {
        self.record(RecordedCall::ApproveMr {
            repo: input.repo.slug(),
            mr_iid: input.mr_iid.to_string(),
            head_sha: input.head_sha.to_string(),
            agent_id: input.agent_id.to_string(),
            dry_run: input.dry_run,
        });
        if self.should_fail("approve_mr") {
            return Err(HostError::Transient("forced fail: approve_mr".into()));
        }
        Ok(format!("fake-approve-{}", input.mr_iid))
    }

    async fn list_open_prs(&self, repo: &RepoRef) -> Result<Vec<PrSummary>, HostError> {
        self.record(RecordedCall::ListOpenPrs { repo: repo.slug() });
        if self.should_fail("list_open_prs") {
            return Err(HostError::Transient("forced fail: list_open_prs".into()));
        }
        // Unknown repos return an empty list — gracefully, since "no PRs"
        // is a legitimate poll outcome and forces consumers to handle it.
        let open_prs = self.open_prs.lock().unwrap();
        Ok(match open_prs.get(&repo.slug()).cloned() {
            Some(prs) => prs,
            None => Vec::new(),
        })
    }

    async fn get_pr_state(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrLiveState, HostError> {
        self.record(RecordedCall::GetPrState {
            repo: repo.slug(),
            mr_iid: mr_iid.to_string(),
        });
        if self.should_fail("get_pr_state") {
            return Err(HostError::Transient("forced fail: get_pr_state".into()));
        }
        match self
            .pr_states
            .lock()
            .unwrap()
            .get(&(repo.slug(), mr_iid.to_string()))
        {
            Some(s) => Ok(s.clone()),
            // Unknown PRs surface as `NotImplemented`. The daemon must
            // never silently treat a missing PR as "fresh" — it has to
            // distinguish "no state yet" from "host says still open".
            None => Err(HostError::NotImplemented),
        }
    }

    async fn fetch_pr_diff(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrDiff, HostError> {
        self.record(RecordedCall::FetchPrDiff {
            repo: repo.slug(),
            mr_iid: mr_iid.to_string(),
        });
        if self.should_fail("fetch_pr_diff") {
            return Err(HostError::Transient("forced fail: fetch_pr_diff".into()));
        }
        match self
            .pr_diffs
            .lock()
            .unwrap()
            .get(&(repo.slug(), mr_iid.to_string()))
        {
            Some(d) => Ok(d.clone()),
            // Unknown diffs surface as a Permanent error rather than
            // `NotImplemented`, because "the diff isn't seeded" is a
            // test-fixture mistake the test author wants to see, not a
            // legitimate runtime state the consumer should handle.
            None => Err(HostError::Permanent(format!(
                "no diff seeded for {}#{}",
                repo.slug(),
                mr_iid
            ))),
        }
    }

    async fn fetch_target_policy_sha(
        &self,
        repo: &RepoRef,
        target_branch: &str,
    ) -> Result<Option<String>, HostError> {
        self.record(RecordedCall::FetchTargetPolicySha {
            repo: repo.slug(),
            target_branch: target_branch.to_string(),
        });
        if self.should_fail("fetch_target_policy_sha") {
            return Err(HostError::Transient(
                "forced fail: fetch_target_policy_sha".into(),
            ));
        }
        // Unseeded entries fall through to Ok(None) — same contract as
        // the trait default impl, so unseeded fakes mirror "host says
        // no policy on the target branch".
        Ok(self
            .target_policy_shas
            .lock()
            .unwrap()
            .get(&(repo.slug(), target_branch.to_string()))
            .cloned()
            .unwrap_or(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_summary(iid: &str, head: &str) -> PrSummary {
        PrSummary {
            mr_iid: iid.into(),
            head_sha: head.into(),
            target_branch: "main".into(),
            author: "octocat".into(),
            title: "test PR".into(),
            draft: false,
            labels: vec![],
        }
    }

    fn sample_state(iid: &str, head: &str, base_sha: &str) -> PrLiveState {
        PrLiveState {
            mr_iid: iid.into(),
            head_sha: head.into(),
            target_branch: "main".into(),
            target_branch_sha: base_sha.into(),
            target_policy_sha: None,
            fetched_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn fake_githost_records_ping_user_call() {
        let f = FakeGitHost::new();
        let id = f.ping_user().await.expect("ping");
        assert_eq!(id.login, "fake-bot");
        assert_eq!(f.calls(), vec![RecordedCall::PingUser]);
    }

    #[tokio::test]
    async fn fake_githost_records_list_open_prs_call_with_repo_slug() {
        let f = FakeGitHost::new();
        let r = RepoRef::parse("octo/widget").unwrap();
        let _ = f.list_open_prs(&r).await.expect("list");
        assert_eq!(
            f.calls(),
            vec![RecordedCall::ListOpenPrs {
                repo: "octo/widget".into()
            }]
        );
    }

    #[tokio::test]
    async fn fake_githost_returns_seeded_prs_for_repo() {
        let s = sample_summary("3", "head3");
        let f = FakeGitHost::new().with_open_prs("octo/widget", vec![s.clone()]);
        let r = RepoRef::parse("octo/widget").unwrap();
        let prs = f.list_open_prs(&r).await.expect("list");
        assert_eq!(prs, vec![s]);

        // Unknown repo returns an empty list gracefully.
        let other = RepoRef::parse("octo/other").unwrap();
        let empty = f.list_open_prs(&other).await.expect("list");
        assert!(empty.is_empty());
    }

    #[tokio::test]
    async fn fake_githost_returns_seeded_state_for_pr() {
        let st = sample_state("3", "head3", "base3");
        let f = FakeGitHost::new().with_pr_state("octo/widget", "3", st.clone());
        let r = RepoRef::parse("octo/widget").unwrap();
        let got = f.get_pr_state(&r, "3").await.expect("get");
        assert_eq!(got, st);
    }

    #[tokio::test]
    async fn fake_githost_fail_on_returns_transient_error() {
        let f = FakeGitHost::new().fail_next("list_open_prs");
        let r = RepoRef::parse("octo/widget").unwrap();
        let err = f.list_open_prs(&r).await.expect_err("must fail once");
        assert!(matches!(err, HostError::Transient(_)));
        // Call still recorded so callers can assert intent.
        assert_eq!(
            f.calls(),
            vec![RecordedCall::ListOpenPrs {
                repo: "octo/widget".into()
            }]
        );
        // Flag clears after firing once: the second call now succeeds.
        let ok = f.list_open_prs(&r).await.expect("second call ok");
        assert!(ok.is_empty());
    }

    #[tokio::test]
    async fn fake_githost_unknown_pr_returns_not_implemented_err() {
        let f = FakeGitHost::new();
        let r = RepoRef::parse("octo/widget").unwrap();
        let err = f.get_pr_state(&r, "999").await.expect_err("unknown");
        assert!(matches!(err, HostError::NotImplemented));
    }

    #[tokio::test]
    async fn pr_live_state_fetched_at_is_recent() {
        // The fake's seeded state uses `Utc::now()` at construction; we
        // assert it lands within a generous window so this test is robust
        // on slow CI but still catches "the fixture stamped 1970-01-01".
        let before = chrono::Utc::now();
        let st = sample_state("1", "h", "b");
        let after = chrono::Utc::now();
        assert!(
            st.fetched_at >= before && st.fetched_at <= after,
            "fetched_at {:?} must be between {:?} and {:?}",
            st.fetched_at,
            before,
            after
        );
        // And round-tripping through FakeGitHost preserves the timestamp.
        let f = FakeGitHost::new().with_pr_state("o/w", "1", st.clone());
        let r = RepoRef::parse("o/w").unwrap();
        let got = f.get_pr_state(&r, "1").await.expect("get");
        assert_eq!(got.fetched_at, st.fetched_at);
    }

    fn sample_diff(repo: &str, iid: &str, head: &str, base: &str) -> PrDiff {
        PrDiff {
            repo: repo.into(),
            mr_iid: iid.into(),
            head_sha: head.into(),
            base_sha: base.into(),
            changed_files: vec![crate::git_host::ChangedFileDiff {
                path: "src/foo.rs".into(),
                lines_added: 10,
                lines_removed: 2,
                hunks: vec!["@@ -1,2 +1,10 @@\n some context\n".into()],
            }],
            fetched_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn fake_githost_records_fetch_pr_diff_call() {
        let d = sample_diff("octo/widget", "3", "head3", "base3");
        let f = FakeGitHost::new().with_pr_diff("octo/widget", "3", d);
        let r = RepoRef::parse("octo/widget").unwrap();
        let _ = f.fetch_pr_diff(&r, "3").await.expect("fetch");
        assert_eq!(
            f.calls(),
            vec![RecordedCall::FetchPrDiff {
                repo: "octo/widget".into(),
                mr_iid: "3".into(),
            }]
        );
    }

    #[tokio::test]
    async fn fake_githost_returns_seeded_diff_for_pr() {
        let d = sample_diff("octo/widget", "3", "head3", "base3");
        let f = FakeGitHost::new().with_pr_diff("octo/widget", "3", d.clone());
        let r = RepoRef::parse("octo/widget").unwrap();
        let got = f.fetch_pr_diff(&r, "3").await.expect("fetch");
        assert_eq!(got, d);
    }

    #[tokio::test]
    async fn fake_githost_fail_on_fetch_pr_diff_returns_transient() {
        let f = FakeGitHost::new().fail_next("fetch_pr_diff");
        let r = RepoRef::parse("octo/widget").unwrap();
        let err = f.fetch_pr_diff(&r, "3").await.expect_err("must fail once");
        assert!(matches!(err, HostError::Transient(_)));
        // Call still recorded even on failure (parity with other methods).
        assert_eq!(
            f.calls(),
            vec![RecordedCall::FetchPrDiff {
                repo: "octo/widget".into(),
                mr_iid: "3".into(),
            }]
        );
    }

    #[tokio::test]
    async fn fake_githost_unknown_diff_returns_err() {
        let f = FakeGitHost::new();
        let r = RepoRef::parse("octo/widget").unwrap();
        let err = f.fetch_pr_diff(&r, "999").await.expect_err("unknown");
        match err {
            HostError::Permanent(msg) => {
                assert!(
                    msg.contains("no diff seeded for "),
                    "expected fixture-mistake hint, got: {msg}"
                );
                assert!(
                    msg.contains("octo/widget") && msg.contains("999"),
                    "expected slug+iid in error message, got: {msg}"
                );
            }
            other => panic!("expected Permanent, got {other:?}"),
        }
    }

    // --- Wave 7.B: target_policy_sha surface on the fake ---------------

    #[tokio::test]
    async fn fake_githost_records_fetch_target_policy_sha_call() {
        let f = FakeGitHost::new();
        let r = RepoRef::parse("octo/widget").unwrap();
        let _ = f
            .fetch_target_policy_sha(&r, "main")
            .await
            .expect("default ok");
        assert_eq!(
            f.calls(),
            vec![RecordedCall::FetchTargetPolicySha {
                repo: "octo/widget".into(),
                target_branch: "main".into(),
            }]
        );
    }

    #[tokio::test]
    async fn fake_githost_returns_seeded_target_policy_sha() {
        let f = FakeGitHost::new().with_target_policy_sha(
            "octo/widget",
            "main",
            Some("sha256:cafef00d".into()),
        );
        let r = RepoRef::parse("octo/widget").unwrap();
        let got = f
            .fetch_target_policy_sha(&r, "main")
            .await
            .expect("seeded ok");
        assert_eq!(got.as_deref(), Some("sha256:cafef00d"));
        // Different branch — not seeded — must fall through to None,
        // proving the key is `(repo, target_branch)` and not just repo.
        let other = f
            .fetch_target_policy_sha(&r, "develop")
            .await
            .expect("unseeded ok");
        assert!(other.is_none(), "unseeded branch must not leak across keys");
    }

    #[tokio::test]
    async fn fake_githost_unseeded_target_policy_sha_returns_none() {
        let f = FakeGitHost::new();
        let r = RepoRef::parse("octo/widget").unwrap();
        let got = f
            .fetch_target_policy_sha(&r, "main")
            .await
            .expect("default ok");
        assert!(
            got.is_none(),
            "an unseeded fake must mirror the trait default Ok(None)"
        );
    }

    #[tokio::test]
    async fn fake_githost_records_other_methods_too() {
        // Sanity check the rest of the surface so a future trait method
        // addition doesn't silently bit-rot the fake.
        let f = FakeGitHost::new();
        let repo = RepoRef::parse("o/w").unwrap();
        f.post_check_run(CheckRun {
            repo: &repo,
            head_sha: "abc",
            name: "fake-check",
            status: CheckStatus::Success,
            summary: "ok",
            details_url: None,
            output_text: None,
        })
        .await
        .expect("check");
        f.post_mr_comment(&repo, "1", "hi").await.expect("comment");
        f.approve_mr(MrApproval {
            repo: &repo,
            mr_iid: "1",
            head_sha: "abc",
            agent_id: "agent",
            receipt_digest: "sha256:x",
            dry_run: true,
        })
        .await
        .expect("approve");
        let calls = f.calls();
        assert_eq!(calls.len(), 3);
        assert!(matches!(calls[0], RecordedCall::PostCheckRun { .. }));
        assert!(matches!(calls[1], RecordedCall::PostMrComment { .. }));
        assert!(matches!(calls[2], RecordedCall::ApproveMr { .. }));
    }
}
