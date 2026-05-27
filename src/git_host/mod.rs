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
pub mod gitlab;
pub mod gitlab_stub;
pub mod test_utils;

pub use codeowners::{CodeOwners, CodeOwnersCheck};
pub use github::GitHubClient;
pub use gitlab::GitLabClient;
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
    /// 409-style conflict from the host (e.g. GitLab MR merge SHA mismatch).
    /// Distinct from `Permanent` so service callers can surface
    /// `merge_sha_stale` per §35.9 item 7 without string-matching on `409`.
    #[error("conflict: {0}")]
    Conflict(String),
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

// ---------------------------------------------------------------------------
// W-H-01: GitHost trait expansion — repo lifecycle, browse, settings, MR,
// review, and CI surfaces. These structs and the trait methods that consume
// them are added per `WEB_WORK_CLAUDE.md` §7.3 W-H-01 / §35.1.18 and FINAL
// spec §6.8. New trait methods all have default impls returning
// `HostError::NotImplemented` so existing adapters continue to compile.
// Subsequent W-H-02..07 packages (GitLab + GitHub implementations) override
// them per-host.
// ---------------------------------------------------------------------------

/// Cursor-style pagination input passed into list endpoints.
///
/// `per_page` is best-effort; hosts may clamp to their own maximum (GitHub
/// caps at 100, GitLab at 100). `cursor` is opaque to the caller and
/// originates from a previous `PageResult::next_cursor`.
#[derive(Debug, Clone)]
pub struct Page {
    pub per_page: u32,
    pub cursor: Option<String>,
}

/// One page of results from a list endpoint plus the cursor used to fetch
/// the next page (or `None` when exhausted). `total` is optional because
/// GitHub does not return total counts on most list endpoints; GitLab does
/// via `X-Total`.
#[derive(Debug, Clone)]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

/// Host's view of a repository, returned by `get_repository`,
/// `list_repositories`, `create_repository`, and `update_repository_settings`.
///
/// `provider_etag` is the host's HTTP ETag for this resource (used for
/// optimistic-concurrency settings patches via `If-Match`, per §35.7 of
/// the plan). `default_ref_sha` is the SHA of the tip of `default_branch`
/// at fetch time (best-effort; hosts may not include it).
#[derive(Debug, Clone)]
pub struct HostRepository {
    pub repo: RepoRef,
    pub id: String,
    pub description: Option<String>,
    pub visibility: String,
    pub default_branch: String,
    pub archived: bool,
    pub topics: Vec<String>,
    pub clone_http_url: Option<String>,
    pub clone_ssh_url: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub web_url: Option<String>,
    pub fork: bool,
    pub template: bool,
    pub default_ref_sha: Option<String>,
    pub provider_etag: Option<String>,
}

/// Input to `create_repository`. Borrowed for the lifetime of the call.
#[derive(Debug, Clone)]
pub struct CreateHostRepository<'a> {
    pub owner: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub visibility: &'a str,
    pub auto_init: bool,
    pub gitignore_template: Option<&'a str>,
    pub license_template: Option<&'a str>,
    pub default_branch: Option<&'a str>,
}

/// Settings patch for `update_repository_settings`.
///
/// `description` and `homepage` use `Option<Option<&'a str>>` (RFC 7396
/// merge semantics): outer `None` = "field absent from patch, leave as-is";
/// `Some(None)` = "explicitly clear this field"; `Some(Some(_))` = "set to
/// new value". This is the only way to distinguish "do not touch" from
/// "clear to NULL" without sentinel strings.
#[derive(Debug, Clone)]
pub struct HostRepositorySettingsPatch<'a> {
    pub description: Option<Option<&'a str>>,
    pub homepage: Option<Option<&'a str>>,
    pub visibility: Option<&'a str>,
    pub default_branch: Option<&'a str>,
    pub allow_merge_commit: Option<bool>,
    pub allow_squash_merge: Option<bool>,
    pub allow_rebase_merge: Option<bool>,
    pub allow_auto_merge: Option<bool>,
    pub delete_branch_on_merge: Option<bool>,
    pub archived: Option<bool>,
}

/// A branch or tag on the host. `kind` is `"branch"` or `"tag"`.
#[derive(Debug, Clone)]
pub struct HostRef {
    pub name: String,
    pub sha: String,
    pub kind: String,
    pub protected: bool,
}

/// One entry in a directory listing. `kind` is one of `"blob"`, `"tree"`,
/// `"symlink"`, `"submodule"` per the FINAL spec §6.8 vocabulary.
#[derive(Debug, Clone)]
pub struct HostTreeEntry {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub sha: String,
    pub size_bytes: Option<u64>,
}

/// Raw blob contents fetched by `get_blob` / `get_readme`.
///
/// `bytes` is the decoded payload (GitLab returns base64-encoded content;
/// GitHub Contents API also base64). `is_binary` is set by the adapter
/// using a UTF-8 sniff so the renderer can refuse non-text without a
/// second host round-trip. `mime` is the adapter's best guess (typically
/// content-type from the response or a path-extension lookup).
#[derive(Debug, Clone)]
pub struct HostBlob {
    pub path: String,
    pub sha: String,
    pub size_bytes: u64,
    pub bytes: Vec<u8>,
    pub is_binary: bool,
    pub mime: Option<String>,
}

/// Result of `compare_refs(base, head)`. `ahead_by` counts commits on
/// `head` not on `base`; `behind_by` counts the reverse. `files` lists
/// per-file change shape (status + line counts + optional patch).
#[derive(Debug, Clone)]
pub struct HostCompare {
    pub base_sha: String,
    pub head_sha: String,
    pub ahead_by: u32,
    pub behind_by: u32,
    pub files: Vec<HostChangedFile>,
}

/// One file's change shape inside a `HostCompare`. `status` is one of
/// `"added"`, `"modified"`, `"removed"`, `"renamed"`.
#[derive(Debug, Clone)]
pub struct HostChangedFile {
    pub path: String,
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
    pub patch: Option<String>,
}

/// A single commit on the host, returned by `list_commits`.
#[derive(Debug, Clone)]
pub struct HostCommit {
    pub sha: String,
    pub author: String,
    pub message: String,
    pub parent_shas: Vec<String>,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}

/// Per-line blame for a file at a ref, returned by `get_blame`.
#[derive(Debug, Clone)]
pub struct HostBlame {
    pub lines: Vec<HostBlameLine>,
}

/// One line of blame output. `line` is the 1-based current-file line
/// number; `original_line` is the 1-based line number at the commit that
/// introduced the line (matches GitHub's GraphQL blame shape).
#[derive(Debug, Clone)]
pub struct HostBlameLine {
    pub sha: String,
    pub author: String,
    pub line: u32,
    pub original_line: u32,
    pub content: String,
}

/// A review discussion / thread on a MR/PR.
#[derive(Debug, Clone)]
pub struct HostReviewThread {
    pub id: String,
    pub path: Option<String>,
    pub line: Option<u32>,
    pub resolved: bool,
    pub comments: Vec<HostReviewComment>,
}

/// A single review comment inside a `HostReviewThread`.
#[derive(Debug, Clone)]
pub struct HostReviewComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Input for `create_review_comment`. `side` is `"left"` (old/base) or
/// `"right"` (new/head) per the GitHub PR review vocabulary; GitLab
/// adapters map to `old_path`/`new_path`.
#[derive(Debug, Clone)]
pub struct HostReviewCommentInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub body: &'a str,
    pub path: &'a str,
    pub line: u32,
    pub side: &'a str,
}

/// Input for `submit_review`. `event` is `"approve"`, `"request_changes"`,
/// or `"comment"`. `head_sha` is the SHA the reviewer is binding their
/// review to (Tip1 Law 4 — exact-SHA binding survives at this layer).
#[derive(Debug, Clone)]
pub struct HostSubmitReviewInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub head_sha: &'a str,
    pub event: &'a str,
    pub body: Option<&'a str>,
}

/// Result of `submit_review`. `state` is the host's state string (e.g.
/// `"APPROVED"`, `"CHANGES_REQUESTED"`, `"COMMENTED"`).
#[derive(Debug, Clone)]
pub struct HostReview {
    pub id: String,
    pub state: String,
    pub url: Option<String>,
}

/// Input for `merge_mr`. `expected_head_sha` is the exact-SHA fence
/// (server MUST 409 if the MR's head moved). `method` is `"merge"`,
/// `"squash"`, or `"rebase"`.
#[derive(Debug, Clone)]
pub struct HostMergeInput<'a> {
    pub repo: &'a RepoRef,
    pub mr_iid: &'a str,
    pub expected_head_sha: &'a str,
    pub method: &'a str,
    pub commit_title: Option<&'a str>,
    pub commit_message: Option<&'a str>,
}

/// Result of `merge_mr`. `merged` is false when the host accepted the
/// request but the merge is async (e.g. GitLab `merge_when_pipeline_succeeds`).
#[derive(Debug, Clone)]
pub struct HostMergeResult {
    pub merged: bool,
    pub sha: Option<String>,
    pub url: Option<String>,
}

/// A CI pipeline on the host. `status` is one of `"pending"`, `"running"`,
/// `"success"`, `"failure"`, `"canceled"`, `"skipped"`.
#[derive(Debug, Clone)]
pub struct HostPipeline {
    pub id: String,
    pub ref_name: String,
    pub sha: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub web_url: Option<String>,
}

/// One job inside a pipeline.
#[derive(Debug, Clone)]
pub struct HostJob {
    pub id: String,
    pub name: String,
    pub stage: String,
    pub status: String,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_secs: Option<u64>,
}

/// Job log bytes plus a truncation flag (hosts cap large logs; the UI
/// renders a "log was truncated, fetch full" affordance when `truncated`).
#[derive(Debug, Clone)]
pub struct HostJobLog {
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

/// A branch-protection rule. `pattern` is a refname glob (e.g. `main`,
/// `release/*`). `bypass_actors` is a list of principal ids allowed to
/// override the rule (host-specific shape).
#[derive(Debug, Clone)]
pub struct HostBranchProtection {
    pub pattern: String,
    pub required_checks: Vec<String>,
    pub required_approvals: u32,
    pub require_signed_commits: bool,
    pub require_conversation_resolution: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
    pub bypass_actors: Vec<String>,
}

/// Metadata for a CI/CD secret. **Values are NEVER returned** (per
/// §35.1.18 of the plan); the secrets surface exposes name/scope/age only.
/// `fingerprint` is an adapter-provided opaque tag (e.g. last 4 chars of
/// a SHA-256) so the UI can show "value matches" without leaking the
/// actual secret.
#[derive(Debug, Clone)]
pub struct HostSecretMetadata {
    pub name: String,
    pub scope: String,
    pub fingerprint: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_accessed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A configured webhook on the repository.
#[derive(Debug, Clone)]
pub struct HostWebhook {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
}

/// A member/collaborator of the repository. `role` is the host's
/// raw role string (e.g. GitLab `"developer"`, `"maintainer"`); the
/// permissions layer (`src/repos/permissions.rs`, W-CC-07′) maps to the
/// normalized 24-key permission set per §35.1.18.
#[derive(Debug, Clone)]
pub struct HostMember {
    pub id: String,
    pub login: String,
    pub role: String,
    pub name: Option<String>,
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

    // ----------------------------------------------------------------------
    // W-H-01: Repository lifecycle, browse, settings, MR, review, CI surfaces.
    // All methods default to `Err(HostError::NotImplemented)`. Adapters
    // (GitLab, GitHub, stubs, fakes) override what they support; the rest
    // surface as `NotImplemented` and callers (web handlers, daemon) MUST
    // handle that error rather than panicking. See FINAL §6.8 + plan §7.3.
    // ----------------------------------------------------------------------

    /// List repositories visible to the authenticated principal, optionally
    /// scoped to an `owner` (user or group). Adapters paginate via
    /// `Page::cursor`; callers walk pages by passing `next_cursor` back.
    /// No silent truncation: when the underlying host returns more pages,
    /// `next_cursor` MUST be set.
    async fn list_repositories(
        &self,
        _owner: Option<&str>,
        _page: Page,
    ) -> Result<PageResult<HostRepository>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Create a new repository. Web handlers must call this only after the
    /// action layer has cleared the 14-step algorithm (§35.1.14) and the
    /// caller has explicit `dry_run=false`.
    async fn create_repository(
        &self,
        _input: CreateHostRepository<'_>,
    ) -> Result<HostRepository, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Fetch the host's current view of a single repository.
    async fn get_repository(&self, _repo: &RepoRef) -> Result<HostRepository, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Patch repository settings (merge semantics — only fields present in
    /// the patch are sent to the host). Callers should pair with an
    /// `If-Match` ETag (provided in `HostRepository::provider_etag`) to
    /// implement optimistic concurrency at the HTTP layer.
    async fn update_repository_settings(
        &self,
        _repo: &RepoRef,
        _patch: HostRepositorySettingsPatch<'_>,
    ) -> Result<HostRepository, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List branches and tags for `repo`. Adapters MAY paginate internally
    /// to return the full list — callers expect a complete view.
    async fn list_refs(&self, _repo: &RepoRef) -> Result<Vec<HostRef>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List one level of a tree at `ref_name`/`path` (non-recursive).
    /// `path` may be empty for the repo root. Paths MUST be already
    /// normalized by the caller (no `..`, no leading `/`).
    async fn list_tree(
        &self,
        _repo: &RepoRef,
        _ref_name: &str,
        _path: &str,
    ) -> Result<Vec<HostTreeEntry>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Fetch the raw bytes of a single file at `ref_name`/`path`.
    /// Adapters set `HostBlob::is_binary` so renderers can refuse non-text
    /// without a second host round-trip.
    async fn get_blob(
        &self,
        _repo: &RepoRef,
        _ref_name: &str,
        _path: &str,
    ) -> Result<HostBlob, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Fetch the repo's README at `ref_name` (or default branch when
    /// `None`). GitLab tries `README.md`, `README`, `Readme.md`, `readme.md`
    /// in order; GitHub uses its dedicated `/readme` endpoint.
    async fn get_readme(
        &self,
        _repo: &RepoRef,
        _ref_name: Option<&str>,
    ) -> Result<HostBlob, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Compare two refs (`base...head`). Used by the diff cockpit and the
    /// "rebase needed" detector in Merge Passport gate #11.
    async fn compare_refs(
        &self,
        _repo: &RepoRef,
        _base: &str,
        _head: &str,
    ) -> Result<HostCompare, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List commits on `ref_name`, optionally filtered to `path`.
    async fn list_commits(
        &self,
        _repo: &RepoRef,
        _ref_name: &str,
        _path: Option<&str>,
        _page: Page,
    ) -> Result<PageResult<HostCommit>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Get per-line blame for `path` at `ref_name`.
    async fn get_blame(
        &self,
        _repo: &RepoRef,
        _ref_name: &str,
        _path: &str,
    ) -> Result<HostBlame, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List discussion / review threads on a MR/PR.
    async fn list_review_threads(
        &self,
        _repo: &RepoRef,
        _mr_iid: &str,
    ) -> Result<Vec<HostReviewThread>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Post a line-anchored review comment on a MR/PR.
    async fn create_review_comment(
        &self,
        _input: HostReviewCommentInput<'_>,
    ) -> Result<HostReviewComment, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Submit a review (`approve` / `request_changes` / `comment`) with
    /// exact-SHA binding via `HostSubmitReviewInput::head_sha`.
    async fn submit_review(
        &self,
        _input: HostSubmitReviewInput<'_>,
    ) -> Result<HostReview, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Merge a MR/PR. The exact-SHA fence (`expected_head_sha`) MUST be
    /// honored by the adapter — a SHA mismatch returns a host-specific
    /// 409 error mapped to `HostError::Permanent` with the
    /// `merge_sha_stale` code at the HTTP layer (§35.9 item 7).
    async fn merge_mr(&self, _input: HostMergeInput<'_>) -> Result<HostMergeResult, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List CI pipelines for `repo`, optionally filtered to `ref_name`.
    async fn list_pipelines(
        &self,
        _repo: &RepoRef,
        _ref_name: Option<&str>,
        _page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List jobs inside a pipeline.
    async fn list_jobs(
        &self,
        _repo: &RepoRef,
        _pipeline_id: &str,
    ) -> Result<Vec<HostJob>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// Fetch raw log bytes for a CI job. Hosts cap log size; the adapter
    /// sets `HostJobLog::truncated` when bytes were trimmed.
    async fn get_job_log(
        &self,
        _repo: &RepoRef,
        _job_id: &str,
    ) -> Result<HostJobLog, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List branch-protection rules for `repo`.
    async fn list_branch_protections(
        &self,
        _repo: &RepoRef,
    ) -> Result<Vec<HostBranchProtection>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List secret metadata (**values NEVER returned** — §35.1.18).
    async fn list_secrets_metadata(
        &self,
        _repo: &RepoRef,
    ) -> Result<Vec<HostSecretMetadata>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List configured webhooks for `repo`.
    async fn list_webhooks(&self, _repo: &RepoRef) -> Result<Vec<HostWebhook>, HostError> {
        Err(HostError::NotImplemented)
    }

    /// List members / collaborators of `repo` with their host-native role.
    /// Mapping to the normalized 24-key permission set (§35.1.18) is the
    /// responsibility of `src/repos/permissions.rs` (W-CC-07′).
    async fn list_members(&self, _repo: &RepoRef) -> Result<Vec<HostMember>, HostError> {
        Err(HostError::NotImplemented)
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

    // --- W-H-01: expanded trait surface defaults to NotImplemented ---

    fn r() -> RepoRef {
        RepoRef::parse("anthropics/claude-code").unwrap()
    }
    fn page() -> Page {
        Page {
            per_page: 50,
            cursor: None,
        }
    }

    #[tokio::test]
    async fn minimal_host_returns_not_implemented_for_repository_surface() {
        let h = MinimalHost;
        assert!(matches!(
            h.list_repositories(None, page()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.get_repository(&r()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.create_repository(CreateHostRepository {
                owner: "anthropics",
                name: "newrepo",
                description: None,
                visibility: "private",
                auto_init: true,
                gitignore_template: None,
                license_template: None,
                default_branch: Some("main"),
            })
            .await,
            Err(HostError::NotImplemented)
        ));
        let patch = HostRepositorySettingsPatch {
            description: Some(Some("new desc")),
            homepage: None,
            visibility: None,
            default_branch: None,
            allow_merge_commit: None,
            allow_squash_merge: None,
            allow_rebase_merge: None,
            allow_auto_merge: None,
            delete_branch_on_merge: None,
            archived: None,
        };
        assert!(matches!(
            h.update_repository_settings(&r(), patch).await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn minimal_host_returns_not_implemented_for_browse_surface() {
        let h = MinimalHost;
        assert!(matches!(
            h.list_refs(&r()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_tree(&r(), "main", "").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.get_blob(&r(), "main", "README.md").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.get_readme(&r(), None).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.compare_refs(&r(), "main", "feature").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_commits(&r(), "main", None, page()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.get_blame(&r(), "main", "src/lib.rs").await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn minimal_host_returns_not_implemented_for_review_surface() {
        let h = MinimalHost;
        let repo = r();
        assert!(matches!(
            h.list_review_threads(&repo, "1").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.create_review_comment(HostReviewCommentInput {
                repo: &repo,
                mr_iid: "1",
                body: "lgtm",
                path: "src/lib.rs",
                line: 10,
                side: "right",
            })
            .await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.submit_review(HostSubmitReviewInput {
                repo: &repo,
                mr_iid: "1",
                head_sha: "deadbeef",
                event: "approve",
                body: None,
            })
            .await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.merge_mr(HostMergeInput {
                repo: &repo,
                mr_iid: "1",
                expected_head_sha: "deadbeef",
                method: "squash",
                commit_title: None,
                commit_message: None,
            })
            .await,
            Err(HostError::NotImplemented)
        ));
    }

    #[tokio::test]
    async fn minimal_host_returns_not_implemented_for_ci_and_governance_surface() {
        let h = MinimalHost;
        assert!(matches!(
            h.list_pipelines(&r(), None, page()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_jobs(&r(), "p1").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.get_job_log(&r(), "j1").await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_branch_protections(&r()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_secrets_metadata(&r()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_webhooks(&r()).await,
            Err(HostError::NotImplemented)
        ));
        assert!(matches!(
            h.list_members(&r()).await,
            Err(HostError::NotImplemented)
        ));
    }

    // --- W-H-01: model struct construction smoke tests ---

    #[test]
    fn host_repository_constructs_with_all_fields() {
        let now = chrono::Utc::now();
        let h = HostRepository {
            repo: r(),
            id: "12345".into(),
            description: Some("desc".into()),
            visibility: "private".into(),
            default_branch: "main".into(),
            archived: false,
            topics: vec!["foo".into()],
            clone_http_url: Some("https://example.com/x/y.git".into()),
            clone_ssh_url: Some("git@example.com:x/y.git".into()),
            updated_at: Some(now),
            web_url: Some("https://example.com/x/y".into()),
            fork: false,
            template: false,
            default_ref_sha: Some("abc".into()),
            provider_etag: Some("\"etag\"".into()),
        };
        let cloned = h.clone();
        assert_eq!(cloned.id, "12345");
        assert!(!cloned.archived);
        assert_eq!(cloned.topics.len(), 1);
        assert_eq!(cloned.provider_etag.as_deref(), Some("\"etag\""));
    }

    #[test]
    fn host_repository_settings_patch_distinguishes_unset_and_clear() {
        // outer None = absent; Some(None) = clear; Some(Some(_)) = set.
        let patch = HostRepositorySettingsPatch {
            description: Some(None),                // explicit clear
            homepage: None,                         // absent
            visibility: Some("internal"),           // set
            default_branch: None,
            allow_merge_commit: Some(false),
            allow_squash_merge: Some(true),
            allow_rebase_merge: None,
            allow_auto_merge: None,
            delete_branch_on_merge: Some(true),
            archived: Some(false),
        };
        // Verify the three states are distinguishable in the type system.
        assert!(matches!(patch.description, Some(None)));
        assert!(patch.homepage.is_none());
        assert_eq!(patch.visibility, Some("internal"));
        assert_eq!(patch.allow_squash_merge, Some(true));
    }

    #[test]
    fn host_blob_carries_binary_and_mime_hints() {
        let b = HostBlob {
            path: "logo.png".into(),
            sha: "abcd".into(),
            size_bytes: 1024,
            bytes: vec![0x89, 0x50, 0x4E, 0x47],
            is_binary: true,
            mime: Some("image/png".into()),
        };
        assert!(b.is_binary);
        assert_eq!(b.mime.as_deref(), Some("image/png"));
        assert_eq!(b.bytes.len(), 4);
    }

    #[test]
    fn host_compare_carries_ahead_behind_and_files() {
        let c = HostCompare {
            base_sha: "base".into(),
            head_sha: "head".into(),
            ahead_by: 3,
            behind_by: 1,
            files: vec![HostChangedFile {
                path: "src/lib.rs".into(),
                status: "modified".into(),
                additions: 10,
                deletions: 2,
                patch: Some("@@ -1,2 +1,10 @@".into()),
            }],
        };
        assert_eq!(c.ahead_by, 3);
        assert_eq!(c.behind_by, 1);
        assert_eq!(c.files.len(), 1);
        assert_eq!(c.files[0].status, "modified");
    }

    #[test]
    fn host_commit_records_parents_and_timestamp() {
        let now = chrono::Utc::now();
        let c = HostCommit {
            sha: "deadbeef".into(),
            author: "octocat".into(),
            message: "init".into(),
            parent_shas: vec!["aaa".into(), "bbb".into()],
            committed_at: now,
        };
        assert_eq!(c.parent_shas.len(), 2, "merge commits have 2+ parents");
        assert_eq!(c.committed_at, now);
    }

    #[test]
    fn host_blame_line_carries_original_line_numbers() {
        let line = HostBlameLine {
            sha: "abc".into(),
            author: "octocat".into(),
            line: 42,
            original_line: 7,
            content: "fn main() {}".into(),
        };
        assert_eq!(line.line, 42);
        assert_eq!(line.original_line, 7);
        let blame = HostBlame {
            lines: vec![line.clone(), line],
        };
        assert_eq!(blame.lines.len(), 2);
    }

    #[test]
    fn host_pipeline_and_job_have_optional_timing() {
        let now = chrono::Utc::now();
        let p = HostPipeline {
            id: "p1".into(),
            ref_name: "main".into(),
            sha: "deadbeef".into(),
            status: "running".into(),
            created_at: now,
            updated_at: now,
            web_url: Some("https://gitlab.example/x/y/-/pipelines/1".into()),
        };
        assert_eq!(p.status, "running");
        let j_unstarted = HostJob {
            id: "j1".into(),
            name: "build".into(),
            stage: "build".into(),
            status: "pending".into(),
            started_at: None,
            finished_at: None,
            duration_secs: None,
        };
        assert!(j_unstarted.started_at.is_none());
        let j_done = HostJob {
            id: "j2".into(),
            name: "test".into(),
            stage: "test".into(),
            status: "success".into(),
            started_at: Some(now),
            finished_at: Some(now),
            duration_secs: Some(42),
        };
        assert_eq!(j_done.duration_secs, Some(42));
    }

    #[test]
    fn host_job_log_carries_truncated_flag() {
        let l = HostJobLog {
            bytes: b"log line 1\nlog line 2\n".to_vec(),
            truncated: true,
        };
        assert!(l.truncated);
        assert!(l.bytes.len() > 0);
    }

    #[test]
    fn host_branch_protection_carries_required_checks_and_bypass() {
        let bp = HostBranchProtection {
            pattern: "main".into(),
            required_checks: vec!["ci/build".into(), "vibegate/merge-passport".into()],
            required_approvals: 2,
            require_signed_commits: true,
            require_conversation_resolution: true,
            allow_force_pushes: false,
            allow_deletions: false,
            bypass_actors: vec!["admin-group".into()],
        };
        assert_eq!(bp.required_approvals, 2);
        assert!(bp.required_checks.contains(&"vibegate/merge-passport".into()));
        assert!(!bp.allow_force_pushes);
    }

    #[test]
    fn host_secret_metadata_never_carries_values() {
        // Compile-time guard: the struct simply has no `value` field;
        // construct one and confirm only metadata is exposed.
        let s = HostSecretMetadata {
            name: "DATABASE_URL".into(),
            scope: "repo".into(),
            fingerprint: Some("ab12".into()),
            updated_at: Some(chrono::Utc::now()),
            last_accessed_at: None,
        };
        // The only "value-shaped" thing is `fingerprint`, which is an
        // adapter-provided opaque tag, not the actual secret value.
        assert_eq!(s.fingerprint.as_deref(), Some("ab12"));
    }

    #[test]
    fn host_webhook_member_and_ref_construct() {
        let w = HostWebhook {
            id: "wh1".into(),
            url: "https://example.com/hook".into(),
            events: vec!["push".into(), "merge_request".into()],
            active: true,
        };
        assert_eq!(w.events.len(), 2);
        let m = HostMember {
            id: "u1".into(),
            login: "octocat".into(),
            role: "developer".into(),
            name: Some("Octo Cat".into()),
        };
        assert_eq!(m.role, "developer");
        let h = HostRef {
            name: "main".into(),
            sha: "abc".into(),
            kind: "branch".into(),
            protected: true,
        };
        assert!(h.protected);
        assert_eq!(h.kind, "branch");
    }

    #[test]
    fn page_result_carries_cursor_and_total() {
        let pr = PageResult {
            items: vec![1, 2, 3],
            next_cursor: Some("page=2".into()),
            total: Some(42),
        };
        assert_eq!(pr.items.len(), 3);
        assert_eq!(pr.next_cursor.as_deref(), Some("page=2"));
        assert_eq!(pr.total, Some(42));
    }
}
