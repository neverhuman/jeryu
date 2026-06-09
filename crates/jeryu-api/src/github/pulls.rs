//! Pull request routes (`/repos/{owner}/{repo}/pulls...`) and their
//! GitHub-shaped renderers.

use jeryu_core::{
    ChangeSet, CreatePullRequestRequest, ForgeError, MergePullRequestRequest, MergeReadiness,
    OpenPr, OverlapConfig, OverlapDecision, PullRequest, PullRequestState, decide,
};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    Pagination, PullStateSelector, actor, docs_url, error_response, json_response,
    json_response_with_headers, owner_json, paginate, parse_body, parse_number,
};

/// Response header stamped when a create-PR request is hot-fixed onto an
/// existing open PR instead of opening a fresh one.
const HDR_REUSED_PR: &str = "X-Jeryu-Reused-PR";

/// The base SHA the forge assigns when a create request omits `base_sha`.
/// Mirrored here so the overlap engine compares the proposed change against
/// existing PRs on the same default base (see `ForgeCore::create_pull_request`).
const DEFAULT_BASE_SHA: &str = "base";

impl GithubRouter {
    pub(super) fn list_pulls(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
        pull_state: PullStateSelector,
    ) -> Response {
        // The engine's `state_filter` is an exact match on one of its many
        // internal lifecycle states (Mergeable, BlockedByChecks, ...), so it
        // cannot express GitHub's coarse open/closed/all selector on its own: a
        // healthy PR re-evaluates to a richer state on read and would slip past
        // an exact-`Open` filter. So list everything and keep the PRs whose
        // GitHub-rendered `state` field matches the selector, which guarantees
        // the filter agrees with the `state` value each PR reports. Absent or
        // unrecognized `?state=` defaults to `open` (GitHub's documented
        // default), so a bare list now returns only open PRs.
        match self.core.list_pull_requests(owner, repo, None) {
            Ok(pulls) => {
                let body: Vec<Value> = pulls
                    .iter()
                    .filter(|pr| pull_state.keeps(pr_open_or_closed(&pr.state)))
                    .map(pull_request_json)
                    .collect();
                paginate(path, page, &body, |slice, _total| {
                    Value::Array(slice.to_vec())
                })
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_pull(&self, owner: &str, repo: &str, body: &str) -> Response {
        #[cfg_attr(not(feature = "web"), allow(unused_mut))]
        let mut req: CreatePullRequestRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);

        // With a git backend wired, persist the REAL commit oids of the head and
        // base branch refs (never the literal "base"/"head-<n>" placeholders).
        // A request that already carries explicit shas is honored as-is; an
        // unresolvable branch falls through to the core default, preserving the
        // git-less in-memory path used by unit tests.
        #[cfg(feature = "web")]
        if let Some(rm) = &self.repo_manager {
            self.resolve_create_oids(rm, owner, repo, &mut req);
        }

        // Flagship overlap routing: before opening a fresh PR, see whether the
        // proposed change overlaps an existing OPEN PR enough to hot-fix it.
        // Only runs when the request carries `changed_files`; without them there
        // is nothing to score, so we fall through to a normal create.
        if !req.changed_files.is_empty()
            && let Some(response) = self.maybe_route_overlap(owner, repo, &author, &req)
        {
            return response;
        }

        match self.core.create_pull_request(owner, repo, &author, req) {
            Ok(pr) => {
                #[cfg(feature = "web")]
                if let Some(repo_manager) = &self.repo_manager {
                    crate::ci_bridge::seed_pull_request_head(
                        &self.core,
                        repo_manager,
                        owner,
                        repo,
                        &format!("refs/heads/{}", pr.head.ref_name),
                        &pr.head.sha,
                        &crate::ci_bridge::default_origin_base_url(),
                    );
                }
                json_response(201, &pull_request_json(&pr))
            }
            Err(err) => error_response(err),
        }
    }

    /// Fill in a create request's `head_sha`/`base_sha` from the real commit
    /// oids of the corresponding branch refs in the bare repo, but only for a
    /// field the caller left unset and a ref that actually resolves. A git error
    /// is non-fatal here: create then falls back to the core default rather than
    /// failing the whole request on a transient resolve hiccup.
    #[cfg(feature = "web")]
    fn resolve_create_oids(
        &self,
        rm: &std::sync::Arc<jeryu_gitd::RepoManager>,
        owner: &str,
        repo: &str,
        req: &mut CreatePullRequestRequest,
    ) {
        use jeryu_gitd::refs::RefService;

        let Ok(resolved) = rm.resolve_parts(owner, repo) else {
            return;
        };
        let refs = RefService::new((**rm).clone());
        if req.head_sha.is_none()
            && let Ok(Some(oid)) =
                refs.resolve_commit(&resolved, &format!("refs/heads/{}", req.head))
        {
            req.head_sha = Some(oid);
        }
        if req.base_sha.is_none()
            && let Ok(Some(oid)) =
                refs.resolve_commit(&resolved, &format!("refs/heads/{}", req.base))
        {
            req.base_sha = Some(oid);
        }
    }

    /// Runs the PR-overlap engine for a proposed change. Returns:
    /// * `Some(route_to_existing 200)` with an `X-Jeryu-Reused-PR` header when
    ///   the change should hot-fix an existing open PR,
    /// * `Some(409)` when the best candidate overlaps but coalescing is unsafe
    ///   (stale base / unproven head),
    /// * `None` when a fresh PR should be created (caller proceeds as normal).
    ///
    /// Any failure to list the repo's open PRs is treated as "no candidates"
    /// (returns `None`) so overlap routing can never block a legitimate create.
    fn maybe_route_overlap(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        req: &CreatePullRequestRequest,
    ) -> Option<Response> {
        // List every PR and keep the ones GitHub would render as `open`. We do
        // NOT filter by `PullRequestState::Open` at the engine: a healthy PR is
        // re-evaluated to a richer lifecycle state (e.g. `Mergeable`) on read,
        // so an exact-`Open` filter would miss live candidates. Only terminal
        // Merged/Closed PRs are excluded.
        let open_prs = self.core.list_pull_requests(owner, repo, None).ok()?;

        let open: Vec<OpenPr> = open_prs
            .iter()
            .filter(|pr| {
                !matches!(
                    pr.state,
                    PullRequestState::Merged | PullRequestState::Closed
                ) && !pr.merged
            })
            .map(|pr| {
                OpenPr::new(
                    pr.number,
                    pr.changed_files.clone(),
                    pr.base.sha.clone(),
                    // A PR is only safe to coalesce onto if its head currently
                    // evaluates as mergeable (checks/protection green).
                    pr.mergeable,
                )
            })
            .collect();

        if open.is_empty() {
            return None;
        }

        let base_sha = req
            .base_sha
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_SHA.to_string());
        let change = ChangeSet::new(
            req.changed_files.clone(),
            base_sha,
            Some(author.to_string()),
        );

        match decide(&change, &open, OverlapConfig::default()) {
            OverlapDecision::RouteToExisting { pr, reason } => {
                let payload = json!({
                    "route_to_existing": {
                        "pr": pr,
                        "reason": reason,
                    },
                    "message": format!(
                        "change coalesced onto existing pull request #{pr}; no new PR created"
                    ),
                    "documentation_url": docs_url(),
                });
                Some(json_response_with_headers(
                    200,
                    &payload,
                    vec![(HDR_REUSED_PR.to_string(), pr.to_string())],
                ))
            }
            OverlapDecision::RefuseCoalesce { pr, reason } => {
                // GitHub returns 409 Conflict when a change cannot be applied
                // cleanly onto its target; the overlap engine refuses to clobber
                // a stale base or stack work on an unproven head.
                let payload = json!({
                    "message": reason,
                    "refuse_coalesce": { "pr": pr },
                    "documentation_url": docs_url(),
                });
                Some(json_response(409, &payload))
            }
            // CreateNew: nothing safe to coalesce onto; proceed with a fresh PR.
            OverlapDecision::CreateNew { .. } => None,
        }
    }

    pub(super) fn get_pull(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_pull_request(owner, repo, number) {
            Ok(pr) => json_response(200, &pull_request_json(&pr)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn merge_pull(&self, owner: &str, repo: &str, number: &str, body: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let req: MergePullRequestRequest = if body.trim().is_empty() {
            MergePullRequestRequest::default()
        } else {
            match parse_body(body) {
                Ok(value) => value,
                Err(response) => return response,
            }
        };

        // GATE FIRST (no git yet). A blocked PR returns Err(BranchProtection)
        // here and the handler short-circuits BEFORE any git ref is touched.
        let readiness =
            match self
                .core
                .evaluate_merge_readiness(owner, repo, number, req.sha.as_deref())
            {
                Ok(readiness) => readiness,
                // GitHub returns 405 "Method Not Allowed" when a PR is not
                // mergeable (failing checks / protection), distinct from a 404.
                Err(ForgeError::BranchProtection(reason)) => {
                    return json_response(
                        405,
                        &json!({ "message": reason, "documentation_url": docs_url() }),
                    );
                }
                Err(err) => return error_response(err),
            };

        match readiness {
            // Idempotent: an already-merged PR returns its recorded merge sha.
            MergeReadiness::AlreadyMerged { sha } => json_response(
                200,
                &json!({
                    "sha": sha,
                    "merged": true,
                    "message": "Pull Request already merged",
                }),
            ),
            // `base_sha` is intentionally ignored here: the git merge path
            // resolves the LIVE base tip rather than trusting a possibly-stale
            // stored sha. It remains part of the readiness disclosure for
            // callers/inspection.
            MergeReadiness::Ready {
                base_ref,
                head_ref,
                head_sha,
                require_linear_history,
                ..
            } => self.merge_ready_pull(MergeReady {
                owner,
                repo,
                number,
                req: &req,
                base_ref,
                head_ref,
                head_sha,
                require_linear_history,
            }),
        }
    }

    /// Finalize a PR that has already passed the merge gate. With a git
    /// [`RepoManager`] wired this advances the real base ref; otherwise it falls
    /// back to the in-memory synthetic-sha merge.
    fn merge_ready_pull(&self, ready: MergeReady<'_>) -> Response {
        #[cfg(feature = "web")]
        {
            if let Some(rm) = &self.repo_manager {
                return self.merge_ready_pull_git(rm, ready);
            }
            // No git backend wired: production merges silently fall back to the
            // in-memory synthetic-sha path. Surface it so an operator can spot a
            // missing `.with_repo_manager(...)` wiring in web.rs. The crate has
            // no tracing infra, so this matches the existing `eprintln!`
            // advisory-logging convention (see web/tests.rs).
            eprintln!(
                "WARN: repo_manager unset; merge falling back to the in-memory synthetic-sha \
                 path (production git merge not wired)"
            );
        }
        self.merge_ready_pull_in_memory(ready)
    }

    /// Git-less finalize: synthesize a merge sha in core. Used by unit tests and
    /// as the no-`repo_manager` fallback.
    fn merge_ready_pull_in_memory(&self, ready: MergeReady<'_>) -> Response {
        let merge_sha = format!("merge-{}-{}", ready.head_sha, ready.number);
        match self.core.finalize_merge(
            ready.owner,
            ready.repo,
            ready.number,
            merge_sha,
            ready.req.sha.as_deref(),
        ) {
            Ok(result) => merge_success_response(&result),
            Err(ForgeError::BranchProtection(reason)) => json_response(
                405,
                &json!({ "message": reason, "documentation_url": docs_url() }),
            ),
            Err(err) => error_response(err),
        }
    }

    /// Real, gated git merge: advance `refs/heads/{base_ref}` in the bare repo
    /// to the produced merge oid, then reconcile that real sha into the PR
    /// record via `finalize_merge`.
    ///
    /// The merge does NOT trust the PR's stored base/head shas (which may be
    /// stale placeholders): it resolves `base_ref` and the PR's head branch ref
    /// live against the bare repo. When the head is already an ANCESTOR of the
    /// base (its code is in main), it finalizes the record as merged
    /// idempotently without moving any ref. An unresolvable base/head yields a
    /// clean 4xx, never a 500.
    #[cfg(feature = "web")]
    fn merge_ready_pull_git(
        &self,
        rm: &std::sync::Arc<jeryu_gitd::RepoManager>,
        ready: MergeReady<'_>,
    ) -> Response {
        use jeryu_gitd::GitdError;
        use jeryu_gitd::object_fsck::ObjectFsck;
        use jeryu_gitd::refs::RefService;

        let resolved = match rm.resolve_parts(ready.owner, ready.repo) {
            Ok(resolved) => resolved,
            Err(err) => {
                return json_response(
                    500,
                    &json!({
                        "message": format!("could not resolve repository: {err}"),
                        "documentation_url": docs_url(),
                    }),
                );
            }
        };

        let refs = RefService::new((**rm).clone());

        // Resolve the LIVE base tip, never the stored base sha. A base branch
        // that does not resolve is an unprocessable request (422), not a 500.
        let base_oid =
            match refs.resolve_commit(&resolved, &format!("refs/heads/{}", ready.base_ref)) {
                Ok(Some(oid)) => oid,
                Ok(None) => {
                    return json_response(
                        422,
                        &json!({
                            "message": format!(
                                "base ref refs/heads/{} does not resolve to a commit",
                                ready.base_ref
                            ),
                            "documentation_url": docs_url(),
                        }),
                    );
                }
                Err(err) => {
                    return json_response(
                        500,
                        &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
                    );
                }
            };

        // Resolve the LIVE head: prefer the PR's head branch ref, then fall back
        // to the stored head sha ONLY if it is itself a real commit. A head that
        // resolves by neither route is unprocessable (422), not a 500.
        let head_oid = match self.resolve_pull_head(&refs, &resolved, &ready) {
            Ok(Some(oid)) => oid,
            Ok(None) => {
                return json_response(
                    422,
                    &json!({
                        "message": format!(
                            "head ref refs/heads/{} does not resolve to a commit",
                            ready.head_ref
                        ),
                        "documentation_url": docs_url(),
                    }),
                );
            }
            Err(err) => {
                return json_response(
                    500,
                    &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
                );
            }
        };

        // If the head is already contained in the base history, the code has
        // already landed (e.g. a server-side fast-forward that never flipped the
        // record). Mark the PR merged idempotently against the real base oid
        // WITHOUT moving any ref.
        let fsck = ObjectFsck::new(rm.config().git_bin.clone());
        match fsck.is_ancestor(&resolved, &head_oid, &base_oid) {
            Ok(true) => {
                return match self.core.finalize_merge(
                    ready.owner,
                    ready.repo,
                    ready.number,
                    base_oid,
                    ready.req.sha.as_deref(),
                ) {
                    Ok(result) => merge_success_response(&result),
                    Err(ForgeError::BranchProtection(reason)) => json_response(
                        405,
                        &json!({ "message": reason, "documentation_url": docs_url() }),
                    ),
                    Err(err) => error_response(err),
                };
            }
            Ok(false) => {}
            Err(err) => {
                return json_response(
                    500,
                    &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
                );
            }
        }

        let message = merge_message(ready.number, ready.req);
        let outcome = match refs.merge_pull(
            &resolved,
            "system:pr-merge",
            &format!("refs/heads/{}", ready.base_ref),
            &base_oid,
            &head_oid,
            &message,
            ready.require_linear_history,
        ) {
            Ok(outcome) => outcome,
            // A conflicting tree is a 409, as is a refused non-fast-forward
            // merge on a linear-history-protected base.
            Err(GitdError::MergeConflict(detail)) => {
                return json_response(
                    409,
                    &json!({
                        "message": format!("merge conflict: {detail}"),
                        "documentation_url": docs_url(),
                    }),
                );
            }
            Err(err @ GitdError::NonFastForwardRequired) => {
                return json_response(
                    409,
                    &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
                );
            }
            Err(err) => {
                return json_response(
                    500,
                    &json!({ "message": err.to_string(), "documentation_url": docs_url() }),
                );
            }
        };

        // Reconcile the REAL git oid back into the PR record (handler never
        // mutates the model directly).
        match self.core.finalize_merge(
            ready.owner,
            ready.repo,
            ready.number,
            outcome.merge_oid.clone(),
            ready.req.sha.as_deref(),
        ) {
            Ok(result) => merge_success_response(&result),
            // The ref already moved; we do NOT roll it back. A reconciler can
            // detect the divergence by comparing merge_commit_sha vs the ref.
            Err(ForgeError::BranchProtection(reason)) => json_response(
                405,
                &json!({ "message": reason, "documentation_url": docs_url() }),
            ),
            Err(err) => error_response(err),
        }
    }

    /// Resolve a PR's head to a real commit oid for merging.
    ///
    /// Tries the live head branch ref first (`refs/heads/<head_ref>`), then
    /// falls back to the stored head sha ONLY when it is itself a real commit
    /// in the repo. Returns `Ok(None)` when neither resolves, so the caller
    /// renders a 4xx rather than feeding a placeholder into the merge.
    #[cfg(feature = "web")]
    fn resolve_pull_head(
        &self,
        refs: &jeryu_gitd::refs::RefService,
        repo: &jeryu_gitd::repo::Repository,
        ready: &MergeReady<'_>,
    ) -> jeryu_gitd::Result<Option<String>> {
        if let Some(oid) = refs.resolve_commit(repo, &format!("refs/heads/{}", ready.head_ref))? {
            return Ok(Some(oid));
        }
        refs.resolve_commit(repo, &ready.head_sha)
    }
}

/// Parameters describing a PR that has already passed the merge gate.
///
/// `base_ref`/`head_ref`/`require_linear_history` are only consumed by the real
/// git-merge path (`web` feature); the in-memory fallback uses only
/// `head_sha`/`number`.
struct MergeReady<'a> {
    owner: &'a str,
    repo: &'a str,
    number: u64,
    req: &'a MergePullRequestRequest,
    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    base_ref: String,
    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    head_ref: String,
    head_sha: String,
    #[cfg_attr(not(feature = "web"), allow(dead_code))]
    require_linear_history: bool,
}

fn merge_success_response(result: &jeryu_core::MergeResult) -> Response {
    json_response(
        200,
        &json!({
            "sha": result.sha,
            "merged": result.merged,
            "message": result.message,
        }),
    )
}

/// Build the merge commit message: a default GitHub-shaped title, optionally
/// followed by a request-provided title/body.
#[cfg(feature = "web")]
fn merge_message(number: u64, req: &MergePullRequestRequest) -> String {
    let mut message = format!("Merge pull request #{number}");
    if let Some(title) = req
        .commit_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        message.push_str("\n\n");
        message.push_str(title);
    }
    if let Some(body) = req
        .commit_message
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        message.push_str("\n\n");
        message.push_str(body);
    }
    message
}

pub(super) fn pull_request_json(pr: &PullRequest) -> Value {
    json!({
        "id": pr.id,
        // GitHub-compatible: per-repo `number`, never an internal/global id.
        "number": pr.number,
        "state": pr_open_or_closed(&pr.state),
        "draft": pr.draft,
        "title": pr.title,
        "body": pr.body,
        "user": owner_json(&pr.author),
        "head": git_ref_json(pr, &pr.head),
        "base": git_ref_json(pr, &pr.base),
        "mergeable": pr.mergeable,
        "mergeable_state": pr.mergeable_state,
        "merged": pr.merged,
        "merged_at": pr.merged_at,
        "merge_commit_sha": pr.merge_commit_sha,
        "source_repository": pr.source_repository,
        "html_url": format!("/{}/{}/pull/{}", pr.owner, pr.repo, pr.number),
        "url": format!("/repos/{}/{}/pulls/{}", pr.owner, pr.repo, pr.number),
        "created_at": pr.created_at,
        "updated_at": pr.updated_at,
    })
}

fn git_ref_json(pr: &PullRequest, git_ref: &jeryu_core::GitBranchRef) -> Value {
    json!({
        "label": git_ref.label,
        "ref": git_ref.ref_name,
        "sha": git_ref.sha,
        "repo": { "full_name": format!("{}/{}", pr.owner, pr.repo) },
    })
}

/// GitHub PRs only ever report `open`, `closed`, or merged-as-closed at the
/// `state` field. Jeryu's richer lifecycle (Mergeable, BlockedByChecks, ...)
/// is surfaced through `mergeable`/`mergeable_state`; `state` is normalized.
fn pr_open_or_closed(state: &PullRequestState) -> &'static str {
    match state {
        PullRequestState::Merged | PullRequestState::Closed => "closed",
        _ => "open",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::{CreateRepositoryRequest, CreateUserRequest, ForgeCore};

    #[test]
    fn pull_request_json_includes_source_repository() {
        let core = ForgeCore::new();
        core.create_user(CreateUserRequest {
            login: "alice".to_string(),
            ..Default::default()
        })
        .unwrap();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                default_branch: Some("main".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let default_pr = core
            .create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: "default source".to_string(),
                    head: "feature".to_string(),
                    base: "main".to_string(),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            pull_request_json(&default_pr)["source_repository"],
            "alice/jeryu"
        );

        let explicit_pr = core
            .create_pull_request(
                "alice",
                "jeryu",
                "alice",
                CreatePullRequestRequest {
                    title: "forked source".to_string(),
                    head: "feature-2".to_string(),
                    base: "main".to_string(),
                    source_repository: Some("fork-owner/jeryu".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            pull_request_json(&explicit_pr)["source_repository"],
            "fork-owner/jeryu"
        );
    }
}
