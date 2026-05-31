//! Issues and issue comments.

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::{ForgeCore, emit_event_locked, next_issue_number, require_name};
use crate::errors::{ForgeError, Result};
use crate::model::*;
use crate::webhooks::event_payload;

impl ForgeCore {
    pub fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        author: &str,
        request: CreateIssueRequest,
    ) -> Result<Issue> {
        require_name("issue title", &request.title)?;
        self.ensure_repo_exists(owner, repo)?;
        self.ensure_user(author);
        let mut state = self.state.write();
        let number = next_issue_number(&mut state, owner, repo);
        let now = Utc::now();
        let issue = Issue {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
            title: request.title,
            body: request.body,
            state: IssueState::Open,
            author: author.to_string(),
            labels: request.labels,
            assignees: request.assignees,
            milestone: request.milestone,
            comments: 0,
            pull_request: None,
            created_at: now,
            updated_at: now,
            closed_at: None,
        };
        state
            .issues
            .insert((owner.to_string(), repo.to_string(), number), issue.clone());
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issues",
            event_payload("opened", "issue", json!(issue.clone())),
        );
        Ok(issue)
    }

    pub fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state_filter: Option<IssueState>,
    ) -> Result<Vec<Issue>> {
        self.ensure_repo_exists(owner, repo)?;
        let mut issues: Vec<_> = self
            .state
            .read()
            .issues
            .values()
            .filter(|issue| issue.owner == owner && issue.repo == repo)
            .filter(|issue| {
                state_filter
                    .as_ref()
                    .is_none_or(|state| &issue.state == state)
            })
            .cloned()
            .collect();
        issues.sort_by_key(|issue| issue.number);
        Ok(issues)
    }

    pub fn get_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Issue> {
        self.state
            .read()
            .issues
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))
    }

    pub fn update_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        request: UpdateIssueRequest,
    ) -> Result<Issue> {
        let mut state = self.state.write();
        let key = (owner.to_string(), repo.to_string(), number);
        let issue = state
            .issues
            .get_mut(&key)
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))?;
        if let Some(title) = request.title {
            require_name("issue title", &title)?;
            issue.title = title;
        }
        if request.body.is_some() {
            issue.body = request.body;
        }
        if let Some(labels) = request.labels {
            issue.labels = labels;
        }
        if let Some(assignees) = request.assignees {
            issue.assignees = assignees;
        }
        if request.milestone.is_some() {
            issue.milestone = request.milestone;
        }
        let action = if let Some(new_state) = request.state {
            issue.state = new_state.clone();
            issue.closed_at = if new_state == IssueState::Closed {
                Some(Utc::now())
            } else {
                None
            };
            match new_state {
                IssueState::Open => "reopened",
                IssueState::Closed => "closed",
            }
        } else {
            "edited"
        };
        issue.updated_at = Utc::now();
        let updated = issue.clone();
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issues",
            event_payload(action, "issue", json!(updated.clone())),
        );
        Ok(updated)
    }

    pub fn add_issue_comment(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        author: &str,
        request: CreateCommentRequest,
    ) -> Result<IssueComment> {
        require_name("comment body", &request.body)?;
        self.ensure_user(author);
        let mut state = self.state.write();
        let issue_key = (owner.to_string(), repo.to_string(), number);
        let issue = state
            .issues
            .get_mut(&issue_key)
            .ok_or_else(|| ForgeError::NotFound(format!("issue {owner}/{repo}#{number}")))?;
        issue.comments += 1;
        issue.updated_at = Utc::now();
        let comment = IssueComment {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            issue_number: number,
            author: author.to_string(),
            body: request.body,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        state
            .issue_comments
            .entry(issue_key)
            .or_default()
            .push(comment.clone());
        emit_event_locked(
            &mut state,
            owner,
            repo,
            "issue_comment",
            event_payload("created", "comment", json!(comment.clone())),
        );
        Ok(comment)
    }

    pub fn list_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<Vec<IssueComment>> {
        self.get_issue(owner, repo, number)?;
        // The issue exists (checked above); a missing comments entry just means
        // it has no comments yet, so an empty list is the intended value.
        Ok(self
            .state
            .read()
            .issue_comments
            .get(&(owner.to_string(), repo.to_string(), number))
            .cloned()
            .unwrap_or_default())
    }
}
