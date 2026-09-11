//! Pull requests, reviews, and merges.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{
    ForgeCore, apply_evaluation, emit_event_locked, evaluate_locked, next_issue_number,
    next_pull_number, require_name,
};
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::event_payload;

fn normalize_source_repository(
    owner: &str,
    repo: &str,
    source_repository: Option<String>,
) -> String {
    source_repository
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{owner}/{repo}"))
}

/// Compatibility shape for the retired split merge API. No production method
/// returns these variants; use the future Core-owned durable executor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeReadiness {
    /// The PR passed branch protection and is ready to merge.
    Ready {
        /// Base branch name (e.g. `main`).
        base_ref: String,
        /// Current base tip sha to merge into.
        base_sha: String,
        /// Head branch name (e.g. `feature`).
        head_ref: String,
        /// Head sha to merge.
        head_sha: String,
        /// Whether the base requires linear history; the caller must refuse a
        /// non-fast-forward merge when this is set.
        require_linear_history: bool,
    },
    /// The PR is already merged; the recorded merge sha is returned for idempotency.
    AlreadyMerged {
        /// The recorded (or fallback head) sha.
        sha: String,
    },
}

impl ForgeCore {
    pub fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        request: CreatePullRequestRequest,
    ) -> Result<PullRequest> {
        let source_repository = request.source_repository.clone();
        self.with_source_mutation(owner, repo, source_repository.as_deref(), author, || {
            require_name("pull request title", &request.title)?;
            require_name("head", &request.head)?;
            require_name("base", &request.base)?;
            self.ensure_repo_exists(owner, repo)?;
            self.ensure_user_admitted(author)?;
            let mut state = self.runtime.state.write();
            let previous = state.clone();
            let issue_number = next_issue_number(&mut state, owner, repo);
            let pull_number = next_pull_number(&mut state, owner, repo);
            let now = Utc::now();
            let issue = Issue {
                id: Uuid::new_v4(),
                owner: owner.to_string(),
                repo: repo.to_string(),
                number: issue_number,
                title: request.title.clone(),
                body: request.body.clone(),
                state: IssueState::Open,
                author: author.to_string(),
                labels: Vec::new(),
                assignees: Vec::new(),
                milestone: None,
                comments: 0,
                pull_request: Some(PullRequestMarker {
                    url: format!("/repos/{owner}/{repo}/pulls/{pull_number}"),
                    html_url: format!("/{owner}/{repo}/pull/{pull_number}"),
                }),
                created_at: now,
                updated_at: now,
                closed_at: None,
            };
            let mut pr = PullRequest {
                id: Uuid::new_v4(),
                owner: owner.to_string(),
                repo: repo.to_string(),
                number: pull_number,
                issue_number,
                title: request.title,
                body: request.body,
                state: if request.draft {
                    PullRequestState::Draft
                } else {
                    PullRequestState::Open
                },
                draft: request.draft,
                author: author.to_string(),
                source_repository: normalize_source_repository(
                    owner,
                    repo,
                    request.source_repository,
                ),
                head: GitBranchRef::new(
                    request.head,
                    request
                        .head_sha
                        .unwrap_or_else(|| format!("head-{pull_number}")),
                ),
                base: GitBranchRef::new(
                    request.base,
                    request.base_sha.unwrap_or_else(|| "base".to_string()),
                ),
                mergeable: false,
                mergeable_state: "unknown".to_string(),
                merged: false,
                merged_at: None,
                merge_commit_sha: None,
                commits: request.commits,
                changed_files: request.changed_files,
                created_at: now,
                updated_at: now,
            };
            let evaluation = evaluate_locked(&state, &pr, None);
            apply_evaluation(&mut pr, evaluation);
            state
                .issues
                .insert((owner.to_string(), repo.to_string(), issue_number), issue);
            state.pulls.insert(
                (owner.to_string(), repo.to_string(), pull_number),
                pr.clone(),
            );
            emit_event_locked(
                &mut state,
                owner,
                repo,
                "pull_request",
                event_payload("opened", "pull_request", json!(pr.clone())),
            );
            self.persist_after_mutation(&mut state, previous)?;
            Ok(pr)
        })
    }

    pub fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state_filter: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.runtime.state.read();
        let mut pulls: Vec<_> = state
            .pulls
            .values()
            .filter(|pr| pr.owner == owner && pr.repo == repo)
            .filter(|pr| {
                state_filter
                    .as_ref()
                    .is_none_or(|filter| &pr.state == filter)
            })
            .map(|pr| {
                let mut pr = pr.clone();
                let evaluation = evaluate_locked(&state, &pr, None);
                apply_evaluation(&mut pr, evaluation);
                pr
            })
            .collect();
        pulls.sort_by_key(|pr| pr.number);
        Ok(pulls)
    }

    pub fn get_pull_request(&self, owner: &str, repo: &str, number: u64) -> Result<PullRequest> {
        let state = self.runtime.state.read();
        let mut pr = match state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
        {
            Some(pr) => pr,
            None => {
                return Err(ForgeError::NotFound(format!(
                    "pull request {owner}/{repo}#{number}"
                )));
            }
        };
        let evaluation = evaluate_locked(&state, &pr, None);
        apply_evaluation(&mut pr, evaluation);
        Ok(pr)
    }

    pub fn update_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: UpdatePullRequestRequest,
    ) -> Result<PullRequest> {
        self.with_pull_mutation(owner, repo, number, || {
            let mut state = self.runtime.state.write();
            let previous = state.clone();
            let key = (owner.to_string(), repo.to_string(), number);
            let pr = match state.pulls.get_mut(&key) {
                Some(pr) => pr,
                None => {
                    return Err(ForgeError::NotFound(format!(
                        "pull request {owner}/{repo}#{number}"
                    )));
                }
            };
            if let Some(title) = request.title {
                require_name("pull request title", &title)?;
                pr.title = title;
            }
            if request.body.is_some() {
                pr.body = request.body;
            }
            if let Some(draft) = request.draft {
                pr.draft = draft;
                pr.state = if draft {
                    PullRequestState::Draft
                } else {
                    PullRequestState::Open
                };
            }
            if let Some(state_update) = request.state {
                pr.state = state_update;
                pr.draft = pr.state == PullRequestState::Draft;
            }
            if let Some(commits) = request.commits {
                pr.commits = commits;
            }
            if let Some(changed_files) = request.changed_files {
                pr.changed_files = changed_files;
            }
            pr.updated_at = Utc::now();
            let mut updated = pr.clone();
            let evaluation = evaluate_locked(&state, &updated, None);
            apply_evaluation(&mut updated, evaluation);
            state.pulls.insert(key, updated.clone());
            emit_event_locked(
                &mut state,
                owner,
                repo,
                "pull_request",
                event_payload("edited", "pull_request", json!(updated.clone())),
            );
            self.persist_after_mutation(&mut state, previous)?;
            Ok(updated)
        })
    }

    pub fn refresh_pull_request_heads_for_ref(
        &self,
        owner: &str,
        repo: &str,
        ref_name: &str,
        head_sha: &str,
    ) -> Result<Vec<PullRequest>> {
        self.with_repository_mutation(owner, repo, || {
            self.ensure_repo_exists(owner, repo)?;
            let mut state = self.runtime.state.write();
            let previous = state.clone();
            let now = Utc::now();
            let keys: Vec<_> = state
                .pulls
                .iter()
                .filter_map(|(key, pr)| {
                    let open = !pr.merged
                        && !matches!(
                            pr.state,
                            PullRequestState::Closed | PullRequestState::Merged
                        );
                    if pr.owner == owner && pr.repo == repo && pr.head.ref_name == ref_name && open
                    {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect();
            if keys.is_empty() {
                return Ok(Vec::new());
            }

            let mut updated = Vec::new();
            for key in keys {
                if let Some(pr) = state.pulls.get_mut(&key) {
                    pr.head.sha = head_sha.to_string();
                    pr.updated_at = now;
                }
                let Some(mut pr) = state.pulls.get(&key).cloned() else {
                    continue;
                };
                let evaluation = evaluate_locked(&state, &pr, None);
                apply_evaluation(&mut pr, evaluation);
                state.pulls.insert(key, pr.clone());
                emit_event_locked(
                    &mut state,
                    owner,
                    repo,
                    "pull_request",
                    event_payload("synchronize", "pull_request", json!(pr.clone())),
                );
                updated.push(pr);
            }
            self.persist_after_mutation(&mut state, previous)?;
            Ok(updated)
        })
    }

    /// Retired split readiness entrypoint. It refuses before an adapter can
    /// dispatch Git. Authenticated attempts and Core-owned execution are required.
    pub fn evaluate_merge_readiness(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        _requested_sha: Option<&str>,
    ) -> Result<MergeReadiness> {
        self.with_pull_mutation(owner, repo, number, || Err(merge_executor_unavailable()))
    }

    /// Retired caller-supplied result entrypoint. A digest is not evidence that
    /// an authorized Git transaction happened, and cannot close a PR.
    pub fn finalize_merge(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        _merge_sha: String,
        _requested_sha: Option<&str>,
    ) -> Result<MergeResult> {
        self.with_pull_mutation(owner, repo, number, || Err(merge_executor_unavailable()))
    }

    /// Retired Git-less mutation. Synthetic merge results are never produced.
    pub fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        _request: MergePullRequestRequest,
    ) -> Result<MergeResult> {
        self.with_pull_mutation(owner, repo, number, || Err(merge_executor_unavailable()))
    }
}

fn merge_executor_unavailable() -> ForgeError {
    ForgeError::WriterUnavailable(
        "authenticated required-check attempts and Core-owned durable merge execution are unavailable; split merge dispatch is retired".into(),
    )
}
