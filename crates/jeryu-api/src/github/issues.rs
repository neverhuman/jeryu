//! Issue and issue-comment routes (`/repos/{owner}/{repo}/issues...`) and
//! their GitHub-shaped renderers.

use jeryu_core::{CreateCommentRequest, CreateIssueRequest, Issue, IssueComment, IssueState};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{
    Pagination, actor, error_response, json_response, owner_json, paginate, parse_body,
    parse_number,
};

impl GithubRouter {
    pub(super) fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        page: Pagination,
    ) -> Response {
        match self.core.list_issues(owner, repo, None) {
            Ok(issues) => {
                let body: Vec<Value> = issues.iter().map(issue_json).collect();
                paginate(path, page, &body, |slice, _total| {
                    Value::Array(slice.to_vec())
                })
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_issue(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateIssueRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);
        match self.core.create_issue(owner, repo, &author, req) {
            Ok(issue) => json_response(201, &issue_json(&issue)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn list_comments(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.list_issue_comments(owner, repo, number) {
            Ok(comments) => {
                let body: Vec<Value> = comments.iter().map(issue_comment_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_comment(
        &self,
        owner: &str,
        repo: &str,
        number: &str,
        body: &str,
    ) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let req: CreateCommentRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let author = actor(body);
        match self
            .core
            .add_issue_comment(owner, repo, number, &author, req)
        {
            Ok(comment) => json_response(201, &issue_comment_json(&comment)),
            Err(err) => error_response(err),
        }
    }
}

fn issue_json(issue: &Issue) -> Value {
    json!({
        "id": issue.id,
        "number": issue.number,
        "title": issue.title,
        "body": issue.body,
        "state": issue_state(&issue.state),
        "user": owner_json(&issue.author),
        "labels": issue.labels,
        "assignees": issue.assignees.iter().map(|a| owner_json(a)).collect::<Vec<_>>(),
        "comments": issue.comments,
        "pull_request": issue.pull_request.as_ref().map(|marker| json!({
            "url": marker.url,
            "html_url": marker.html_url,
        })),
        "html_url": format!("/{}/{}/issues/{}", issue.owner, issue.repo, issue.number),
        "url": format!("/repos/{}/{}/issues/{}", issue.owner, issue.repo, issue.number),
        "created_at": issue.created_at,
        "updated_at": issue.updated_at,
        "closed_at": issue.closed_at,
    })
}

fn issue_state(state: &IssueState) -> &'static str {
    match state {
        IssueState::Open => "open",
        IssueState::Closed => "closed",
    }
}

fn issue_comment_json(comment: &IssueComment) -> Value {
    json!({
        "id": comment.id,
        "body": comment.body,
        "user": owner_json(&comment.author),
        "url": format!(
            "/repos/{}/{}/issues/comments/{}",
            comment.owner, comment.repo, comment.id
        ),
        "created_at": comment.created_at,
        "updated_at": comment.updated_at,
    })
}
