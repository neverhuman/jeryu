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

impl ForgeCore {
    pub fn create_pull_request(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        request: CreatePullRequestRequest,
    ) -> Result<PullRequest> {
        require_name("pull request title", &request.title)?;
        require_name("head", &request.head)?;
        require_name("base", &request.base)?;
        self.ensure_repo_exists(owner, repo)?;
        self.ensure_user(author);
        let mut state = self.state.write();
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
        Ok(pr)
    }

    pub fn list_pull_requests(
        &self,
        owner: &str,
        repo: &str,
        state_filter: Option<PullRequestState>,
    ) -> Result<Vec<PullRequest>> {
        self.ensure_repo_exists(owner, repo)?;
        let state = self.state.read();
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
        let state = self.state.read();
        let mut pr = state
            .pulls
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
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
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let pr = state
            .pulls
            .get_mut(&key)
            .ok_or_else(|| ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}")))?;
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
        Ok(updated)
    }

    pub fn merge_pull_request(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: MergePullRequestRequest,
    ) -> Result<MergeResult> {
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let pr_snapshot =
            state.pulls.get(&key).cloned().ok_or_else(|| {
                ForgeError::NotFound(format!("pull request {owner}/{repo}#{number}"))
            })?;
        if pr_snapshot.merged {
            return Ok(MergeResult {
                // An already-merged PR normally records its merge commit; fall
                // back to the head sha only for the legacy case where the merge
                // sha was never persisted. This is a real default, not a
                // swallowed error.
                sha: pr_snapshot
                    .merge_commit_sha
                    .unwrap_or_else(|| pr_snapshot.head.sha.clone()),
                merged: true,
                message: "Pull Request already merged".to_string(),
            });
        }
        if pr_snapshot.state == PullRequestState::Closed {
            return Err(ForgeError::Validation(
                "closed pull requests cannot be merged".to_string(),
            ));
        }
        let evaluation = evaluate_locked(&state, &pr_snapshot, request.sha.as_deref());
        if !evaluation.mergeable {
            return Err(ForgeError::BranchProtection(format!(
                "{:?}",
                evaluation.blockers
            )));
        }
        let merge_sha = format!("merge-{}-{}", pr_snapshot.head.sha, number);
        let mut pr = pr_snapshot;
        pr.merged = true;
        pr.state = PullRequestState::Merged;
        pr.mergeable = false;
        pr.mergeable_state = "merged".to_string();
        pr.merged_at = Some(Utc::now());
        pr.merge_commit_sha = Some(merge_sha.clone());
        pr.updated_at = Utc::now();
        state.pulls.insert(key, pr.clone());
        if let Some(issue) =
            state
                .issues
                .get_mut(&(owner.to_string(), repo.to_string(), pr.issue_number))
        {
            issue.state = IssueState::Closed;
            issue.closed_at = Some(Utc::now());
            issue.updated_at = Utc::now();
        }
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "pull_request",
            event_payload("closed", "pull_request", json!(pr)),
        );
        Ok(MergeResult {
            sha: merge_sha,
            merged: true,
            message: "Pull Request successfully merged".to_string(),
        })
    }
}
