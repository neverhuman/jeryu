//! Owner: Evidence Gate / git host adapter plane
//! Proof: `cargo nextest run -p jeryu -- git_host::`
//! Invariants:
//!   - `approve_merge_request` ALWAYS includes an exact-SHA binding (Tip1 Law 4).
//!   - No write call is made unless `dry_run = false` is explicitly requested.
//!   - Auth failures surface as `HostError::Auth` (caller decides next action).
//!
//! Trait-based GitHost surface. GitLab is Codex's territory; this module ships
//! a minimal **GitHub** adapter so the live e2e can exercise check-runs and
//! reviewer comments today using `GITHUB_TOKEN` from the canonical secret
//! chain. The GitLab stub here is intentionally minimal — Codex's Phase 4 work owns the rich
//! MR/approval/status-check/deployment-approval surface.

use async_trait::async_trait;

pub mod codeowners;
pub mod github;
pub mod gitlab_stub;
pub mod test_utils;

pub use codeowners::{CodeOwners, CodeOwnersCheck};
pub use github::GitHubClient;
pub use gitlab_stub::GitLabStubClient;

/// The single canonical name for the GitHub required status check that wraps
/// every internal agent verdict, approval, and hard-stop into ONE visible
/// gate on a PR page.
///
/// Brainstorm reference: **Law 5** in `tips/fullauto/tip1.txt` ("ONE visible
/// required gate") and `tips/fullauto/tip9.txt` ("one visible required gate"
/// doctrine, Wave 5). The whole point is to avoid PR pages getting spammed
/// with one bot check-run per reviewer; instead the orchestrator posts the
/// fused `GateDecision` under this single name. Internal verdict/approval
/// comments are explicitly noise-reduction targets — this constant is the
/// contract.
///
/// Any deviation from this exact string (e.g. `vibegate/passport`,
/// `vibegate-merge-passport`, `merge-passport`) is a spec violation and
/// breaks the org-level required-status-check setup documented in
/// `docs/autonomous-delivery.md` ("Required Check Setup (GitHub)").
pub const VIBEGATE_MERGE_PASSPORT_CHECK_NAME: &str = "vibegate/merge-passport";

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("auth failed (do NOT retry)")]
    Auth,
    #[error("rate limited; retry after {retry_after_ms} ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("transient: {0}")]
    Transient(String),
    #[error("permanent: {0}")]
    Permanent(String),
    #[error("not implemented for this host (use GitLab adapter)")]
    NotImplemented,
}

/// Identifies a repo on a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub owner: String,
    pub name: String,
}

impl RepoRef {
    pub fn parse(slug: &str) -> Option<Self> {
        let (o, n) = slug.split_once('/')?;
        if o.is_empty() || n.is_empty() {
            return None;
        }
        Some(Self {
            owner: o.to_string(),
            name: n.to_string(),
        })
    }
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

/// Authenticated host identity (used by `ping_user`).
#[derive(Debug, Clone)]
pub struct HostIdentity {
    pub login: String,
    pub host: String,
}

/// Check-run / external-status-check input. Status maps to the host's vocab.
#[derive(Debug, Clone)]
pub struct CheckRun<'a> {
    pub repo: &'a RepoRef,
    pub head_sha: &'a str,
    pub name: &'a str, // e.g. "jeryu/vibegate-verdict"
    pub status: CheckStatus,
    pub summary: &'a str,
    pub details_url: Option<&'a str>,
    pub output_text: Option<&'a str>, // markdown
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Queued,
    InProgress,
    Success,
    Failure,
    Neutral,
    ActionRequired, // e.g. RequireHuman
}

#[derive(Debug, Clone)]
pub struct CheckRunResult {
    pub id: String, // host-specific (run id / status id)
    pub url: Option<String>,
}

/// Approval call with exact-SHA binding (Tip1 Law 4).
#[derive(Debug, Clone)]
pub struct MrApproval<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub head_sha: &'a str,
    pub agent_id: &'a str,
    pub receipt_digest: &'a str,
    pub dry_run: bool,
}

/// Cheap summary of an open PR/MR returned by `list_open_prs`.
///
/// Wave 7.C: the autonomy daemon polls this surface to discover PRs the
/// orchestrator needs to consider for evidence-gate work without forcing
/// the daemon to maintain an event subscription or webhook receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSummary {
    pub mr_iid: String,
    pub head_sha: String,
    pub target_branch: String,
    pub author: String,
    pub title: String,
    pub draft: bool,
    pub labels: Vec<String>,
}

/// Live PR state used to decide whether previously-emitted evidence /
/// verdicts are still bound to the same head + base.
///
/// `target_policy_sha` is intentionally `Option<String>` because computing
/// the policy SHA requires reading `.jeryu/autonomy/policies/*.yml` from the
/// target branch, which is more than a single GraphQL/REST call. Wave 8
/// wires that computation; for now adapters return `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrLiveState {
    pub mr_iid: String,
    pub head_sha: String,
    pub target_branch: String,
    pub target_branch_sha: String,
    pub target_policy_sha: Option<String>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

/// One-file slice of a PR diff.
///
/// Wave 8 / auto-rejudge: the `EvidencePackBuilder` calls
/// `GitHost::fetch_pr_diff` to materialize the per-file shape of the PR
/// *as it currently exists on the host* (not as captured in the original
/// evidence pack). Adapters that only have stats may leave `hunks` empty;
/// the GitHub adapter stores the unified-diff patch as a single hunk entry
/// per file (simpler than splitting on `@@`, and the second-pass judge can
/// re-split client-side if it wants per-hunk locality).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFileDiff {
    pub path: String,
    pub lines_added: u32,
    pub lines_removed: u32,
    /// Raw unified-diff hunks for this file (best-effort; may be empty if
    /// the adapter only has stats). Each entry is one hunk header + body,
    /// OR — for the GitHub adapter — the entire `patch` field as a single
    /// string. Documented as best-effort precisely so adapters with poorer
    /// surfaces (stats-only) can comply without lying.
    pub hunks: Vec<String>,
}

/// The full diff for a single PR/MR, captured at `fetched_at`. Used by the
/// Wave 8 auto-rejudge pipeline to build a fresh, signed `EvidencePack`
/// from the host's *current* view of the PR — the original evidence pack
/// may be stale after a force-push or target-branch rebase, and the
/// rejudge primitive needs to reason over today's bytes, not yesterday's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDiff {
    pub repo: String,
    pub mr_iid: String,
    pub head_sha: String,
    pub base_sha: String,
    pub changed_files: Vec<ChangedFileDiff>,
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait GitHost: Send + Sync {
    /// Stable id (e.g. "github", "gitlab").
    fn id(&self) -> &str;
    /// Cheap auth check; never writes.
    async fn ping_user(&self) -> Result<HostIdentity, HostError>;
    /// Post a check-run / status-check on `head_sha`.
    async fn post_check_run(&self, input: CheckRun<'_>) -> Result<CheckRunResult, HostError>;
    /// Post a comment on a MR/PR. `body` should be markdown.
    async fn post_mr_comment(
        &self,
        repo: &RepoRef,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, HostError>;
    /// SHA-bound approval (Tip1 Law 4). GitHub approximates via check-run +
    /// PR review; GitLab supports it natively via `?sha=`.
    async fn approve_mr(&self, input: MrApproval<'_>) -> Result<String, HostError>;
    /// List the currently open PRs/MRs on `repo`. The Wave 7.C autonomy
    /// daemon polls this; adapters should NOT silently truncate (paginate
    /// internally if needed). No default impl on purpose — every adapter
    /// must declare its stance so missing surfaces fail at compile time.
    async fn list_open_prs(&self, repo: &RepoRef) -> Result<Vec<PrSummary>, HostError>;
    /// Fetch live head + base state for a single PR. Used by the daemon to
    /// invalidate verdicts whose `head_sha` or `target_branch_sha` no
    /// longer matches what the host reports.
    async fn get_pr_state(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrLiveState, HostError>;
    /// Fetch the current per-file diff for a PR. Wave 8 / auto-rejudge:
    /// the `EvidencePackBuilder` uses this to materialize a fresh signed
    /// `EvidencePack` from the host's current view of the PR (not from a
    /// cached pack that may now be stale). No default impl on purpose —
    /// every adapter must declare its stance so the auto-rejudge surface
    /// fails at compile time when a new adapter is added.
    async fn fetch_pr_diff(&self, repo: &RepoRef, mr_iid: &str) -> Result<PrDiff, HostError>;

    /// Compute the canonical SHA over `.jeryu/autonomy/policies/*.yml` as they
    /// exist on the protected target branch. This is the surface Wave 8's
    /// auto-rejudge logic uses to detect target-branch policy drift
    /// (Tip1 Law 3: policy MUST be evaluated from the protected target
    /// branch, not from the contributor branch).
    ///
    /// Contract:
    /// - `Ok(None)` means "there is no `.jeryu/autonomy/policies` directory on
    ///   the target branch" (legitimate "no policy" state).
    /// - `Ok(Some("sha256:..."))` is the hex SHA-256 over the concatenated
    ///   policy YAML files in alphabetical filename order, newline-joined.
    /// - `Err(_)` is a host transport problem; the caller (auto-rejudge)
    ///   treats `None` as "policy unknown, assume no drift" and a
    ///   propagated `Err` as something the daemon's normal retry loop
    ///   handles.
    ///
    /// Default impl returns `Ok(None)` so adapters that don't speak the
    /// `.jeryu/autonomy/policies/*.yml` surface (GitLab stub, in-memory fake)
    /// don't break — only `GitHubClient` overrides. Brainstorm reference:
    /// Tip1 Law 3 ("policy evaluated from protected target branch").
    async fn fetch_target_policy_sha(
        &self,
        _repo: &RepoRef,
        _target_branch: &str,
    ) -> Result<Option<String>, HostError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_ref_parses() {
        let r = RepoRef::parse("anthropics/claude-code").unwrap();
        assert_eq!(r.owner, "anthropics");
        assert_eq!(r.name, "claude-code");
        assert_eq!(r.slug(), "anthropics/claude-code");
    }

    #[test]
    fn repo_ref_rejects_bad_slugs() {
        assert!(RepoRef::parse("").is_none());
        assert!(RepoRef::parse("noslash").is_none());
        assert!(RepoRef::parse("/empty").is_none());
        assert!(RepoRef::parse("empty/").is_none());
    }

    #[test]
    fn pr_summary_constructs_with_required_fields() {
        let s = PrSummary {
            mr_iid: "42".into(),
            head_sha: "deadbeef".into(),
            target_branch: "main".into(),
            author: "octocat".into(),
            title: "Wire up the foo".into(),
            draft: false,
            labels: vec!["enhancement".into(), "needs-review".into()],
        };
        assert_eq!(s.mr_iid, "42");
        assert_eq!(s.head_sha, "deadbeef");
        assert_eq!(s.target_branch, "main");
        assert_eq!(s.author, "octocat");
        assert_eq!(s.title, "Wire up the foo");
        assert!(!s.draft);
        assert_eq!(s.labels.len(), 2);
    }

    #[test]
    fn pr_live_state_constructs_with_optional_policy_sha() {
        let now = chrono::Utc::now();
        let s = PrLiveState {
            mr_iid: "7".into(),
            head_sha: "aaa".into(),
            target_branch: "main".into(),
            target_branch_sha: "bbb".into(),
            target_policy_sha: None,
            fetched_at: now,
        };
        assert!(
            s.target_policy_sha.is_none(),
            "Wave 7.C leaves policy_sha unset"
        );
        let s2 = PrLiveState {
            target_policy_sha: Some("policy-sha".into()),
            ..s.clone()
        };
        assert_eq!(s2.target_policy_sha.as_deref(), Some("policy-sha"));
    }

    // --- Wave 7.B: trait-level default impl for fetch_target_policy_sha ---

    /// A bare-bones adapter that overrides nothing must inherit the
    /// `Ok(None)` default from the trait. This guards the trait-extension
    /// contract documented above: existing adapters MUST NOT need code changes.
    struct MinimalHost;

    #[async_trait]
    impl GitHost for MinimalHost {
        fn id(&self) -> &str {
            "minimal"
        }
        async fn ping_user(&self) -> Result<HostIdentity, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn post_check_run(&self, _input: CheckRun<'_>) -> Result<CheckRunResult, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn post_mr_comment(
            &self,
            _repo: &RepoRef,
            _mr_iid: &str,
            _body: &str,
        ) -> Result<String, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn approve_mr(&self, _input: MrApproval<'_>) -> Result<String, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn list_open_prs(&self, _repo: &RepoRef) -> Result<Vec<PrSummary>, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn get_pr_state(
            &self,
            _repo: &RepoRef,
            _mr_iid: &str,
        ) -> Result<PrLiveState, HostError> {
            Err(HostError::NotImplemented)
        }
        async fn fetch_pr_diff(&self, _repo: &RepoRef, _mr_iid: &str) -> Result<PrDiff, HostError> {
            Err(HostError::NotImplemented)
        }
    }

    #[tokio::test]
    async fn default_target_policy_sha_impl_returns_none() {
        let h = MinimalHost;
        let r = RepoRef::parse("anthropics/claude-code").unwrap();
        let got = h
            .fetch_target_policy_sha(&r, "main")
            .await
            .expect("default impl is infallible");
        assert!(
            got.is_none(),
            "trait default must return Ok(None) for adapters that don't override"
        );
    }

    #[test]
    fn pr_live_state_target_policy_sha_round_trips_some_and_none() {
        let now = chrono::Utc::now();
        // None round-trip.
        let none_state = PrLiveState {
            mr_iid: "1".into(),
            head_sha: "h1".into(),
            target_branch: "main".into(),
            target_branch_sha: "b1".into(),
            target_policy_sha: None,
            fetched_at: now,
        };
        let cloned = none_state.clone();
        assert_eq!(cloned, none_state);
        assert!(cloned.target_policy_sha.is_none());

        // Some round-trip.
        let some_state = PrLiveState {
            target_policy_sha: Some("sha256:deadbeef".into()),
            ..none_state.clone()
        };
        let cloned_some = some_state.clone();
        assert_eq!(cloned_some, some_state);
        assert_eq!(
            cloned_some.target_policy_sha.as_deref(),
            Some("sha256:deadbeef")
        );
        // And Some != None on the same other fields.
        assert_ne!(some_state, none_state);
    }
}
