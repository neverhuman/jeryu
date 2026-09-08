//! Issue and issue-comment routes (`/repos/{owner}/{repo}/issues...`) and
//! their GitHub-shaped renderers.

use jeryu_core::{
    CreateCommentRequest, CreateIssueRequest, Issue, IssueComment, IssueState, UpdateIssueRequest,
};
use jeryu_jira::{
    CreateWorkCommentRequest, CreateWorkItemRequest, UpdateWorkItemRequest, WorkError,
    WorkIssueLink, WorkItemKind, WorkPrincipal, WorkPrincipalKind, WorkPriority, WorkRepository,
    WorkStatus,
};
use serde_json::{Value, json};

use crate::routes::Response;

use super::support::{
    Pagination, actor, error_response, json_response, owner_json, paginate, parse_body,
    parse_number,
};
use super::{GithubRouter, WorkBridgeRepair};

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
            Ok(issue) => {
                let mut response = json_response(201, &issue_json(&issue));
                if let Some(repair) = self.link_user_issue_to_work(&issue) {
                    self.record_work_bridge_repair(repair.clone());
                    attach_work_bridge_headers(&mut response, &repair);
                }
                response
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn get_issue(&self, owner: &str, repo: &str, number: &str) -> Response {
        let number = match parse_number(number) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.get_issue(owner, repo, number) {
            Ok(issue) => json_response(200, &issue_json(&issue)),
            Err(err) => error_response(err),
        }
    }

    pub(super) fn update_issue(
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
        let req: UpdateIssueRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.update_issue(owner, repo, number, req) {
            Ok(issue) => {
                let mut response = json_response(200, &issue_json(&issue));
                if let Some(repair) = self.sync_issue_to_work(&issue) {
                    self.record_work_bridge_repair(repair.clone());
                    attach_work_bridge_headers(&mut response, &repair);
                }
                response
            }
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
            Ok(comment) => {
                let mut response = json_response(201, &issue_comment_json(&comment));
                if let Some(repair) = self.sync_issue_comment_to_work(owner, repo, number, &comment)
                {
                    self.record_work_bridge_repair(repair.clone());
                    attach_work_bridge_headers(&mut response, &repair);
                }
                response
            }
            Err(err) => error_response(err),
        }
    }

    fn link_user_issue_to_work(&self, issue: &Issue) -> Option<WorkBridgeRepair> {
        if issue.pull_request.is_some() {
            return None;
        }
        let Some(store) = &self.work_store else {
            return None;
        };
        let repo = self
            .core
            .get_repository(&issue.owner, &issue.repo)
            .ok()
            .map(|repo| WorkRepository {
                id: repo.id.to_string(),
                host: "jeryu".to_string(),
                owner: repo.owner,
                name: repo.name,
            });
        match store.find_by_issue(&issue.owner, &issue.repo, issue.number) {
            Ok(Some(_)) => return None,
            Ok(None) => {}
            Err(error) => {
                return Some(work_bridge_repair(
                    "work_bridge_lookup_failed",
                    "create issue",
                    issue,
                    None,
                    error,
                ));
            }
        }
        let request = CreateWorkItemRequest {
            repo,
            title: issue.title.clone(),
            body: issue.body.clone(),
            status: Some(work_status(&issue.state)),
            kind: Some(kind_from_labels(&issue.labels)),
            priority: Some(priority_from_labels(&issue.labels)),
            labels: issue.labels.clone(),
            assignees: issue
                .assignees
                .iter()
                .map(|login| human_principal(login))
                .collect(),
        };
        let link = WorkIssueLink {
            owner: issue.owner.clone(),
            repo: issue.repo.clone(),
            number: issue.number,
            url: Some(work_issue_url(issue)),
        };
        match store.create_with_issue(request, link) {
            Ok(_) => None,
            Err(error) => Some(work_bridge_repair(
                "work_bridge_create_failed",
                "create issue",
                issue,
                None,
                error,
            )),
        }
    }

    fn sync_issue_to_work(&self, issue: &Issue) -> Option<WorkBridgeRepair> {
        if issue.pull_request.is_some() {
            return None;
        }
        let Some(store) = &self.work_store else {
            return None;
        };
        let work = match store.find_by_issue(&issue.owner, &issue.repo, issue.number) {
            Ok(Some(work)) => work,
            Ok(None) => {
                return Some(work_bridge_repair_message(
                    "work_bridge_missing_item",
                    "update issue",
                    &issue.owner,
                    &issue.repo,
                    issue.number,
                    None,
                    "no mirrored Work item exists for this issue",
                ));
            }
            Err(error) => {
                return Some(work_bridge_repair(
                    "work_bridge_lookup_failed",
                    "update issue",
                    issue,
                    None,
                    error,
                ));
            }
        };
        match store.patch(
            &work.key,
            UpdateWorkItemRequest {
                title: Some(issue.title.clone()),
                body: issue.body.clone(),
                status: Some(work_status(&issue.state)),
                labels: Some(issue.labels.clone()),
                assignees: Some(
                    issue
                        .assignees
                        .iter()
                        .map(|login| human_principal(login))
                        .collect(),
                ),
                ..UpdateWorkItemRequest::default()
            },
        ) {
            Ok(_) => None,
            Err(error) => Some(work_bridge_repair(
                "work_bridge_update_failed",
                "update issue",
                issue,
                Some(work.key),
                error,
            )),
        }
    }

    fn sync_issue_comment_to_work(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        comment: &IssueComment,
    ) -> Option<WorkBridgeRepair> {
        let Some(store) = &self.work_store else {
            return None;
        };
        let issue = match self.core.get_issue(owner, repo, number) {
            Ok(issue) if issue.pull_request.is_some() => return None,
            Ok(issue) => issue,
            Err(error) => {
                return Some(work_bridge_repair_message(
                    "work_bridge_issue_lookup_failed",
                    "create issue comment",
                    owner,
                    repo,
                    number,
                    None,
                    &error.to_string(),
                ));
            }
        };
        let work = match store.find_by_issue(owner, repo, number) {
            Ok(Some(work)) => work,
            Ok(None) => {
                return Some(work_bridge_repair_message(
                    "work_bridge_missing_item",
                    "create issue comment",
                    owner,
                    repo,
                    number,
                    None,
                    "no mirrored Work item exists for this issue comment",
                ));
            }
            Err(error) => {
                return Some(work_bridge_repair(
                    "work_bridge_lookup_failed",
                    "create issue comment",
                    &issue,
                    None,
                    error,
                ));
            }
        };
        match store.add_comment(
            &work.key,
            CreateWorkCommentRequest {
                body: comment.body.clone(),
                author: Some(human_principal(&comment.author)),
            },
        ) {
            Ok(_) => None,
            Err(error) => Some(work_bridge_repair(
                "work_bridge_comment_failed",
                "create issue comment",
                &issue,
                Some(work.key),
                error,
            )),
        }
    }
}

fn human_principal(login: &str) -> WorkPrincipal {
    WorkPrincipal {
        kind: WorkPrincipalKind::Human,
        id: login.to_owned(),
        display_name: None,
    }
}

fn work_status(state: &IssueState) -> WorkStatus {
    match state {
        IssueState::Open => WorkStatus::Ready,
        IssueState::Closed => WorkStatus::Done,
    }
}

fn kind_from_labels(labels: &[String]) -> WorkItemKind {
    if labels.iter().any(|label| label == "bug") {
        WorkItemKind::Bug
    } else if labels.iter().any(|label| label == "docs") {
        WorkItemKind::Docs
    } else if labels.iter().any(|label| label == "ci") {
        WorkItemKind::Ci
    } else {
        WorkItemKind::Task
    }
}

fn priority_from_labels(labels: &[String]) -> WorkPriority {
    if labels.iter().any(|label| label == "p0") {
        WorkPriority::P0
    } else if labels.iter().any(|label| label == "p1") {
        WorkPriority::P1
    } else if labels.iter().any(|label| label == "p3") {
        WorkPriority::P3
    } else if labels.iter().any(|label| label == "p4") {
        WorkPriority::P4
    } else {
        WorkPriority::P2
    }
}

fn attach_work_bridge_headers(response: &mut Response, repair: &WorkBridgeRepair) {
    response
        .headers
        .push(("X-Jeryu-Work-Bridge".to_string(), "degraded".to_string()));
    response
        .headers
        .push(("X-Jeryu-Work-Repair-Code".to_string(), repair.code.clone()));
}

fn work_bridge_repair(
    code: &str,
    operation: &str,
    issue: &Issue,
    work_key: Option<String>,
    error: WorkError,
) -> WorkBridgeRepair {
    let hint = error.repair_hint();
    WorkBridgeRepair {
        code: code.to_string(),
        operation: operation.to_string(),
        owner: issue.owner.clone(),
        repo: issue.repo.clone(),
        issue_number: issue.number,
        work_key,
        reason: error.to_string(),
        common_fixes: hint
            .common_fixes
            .iter()
            .map(|fix| (*fix).to_string())
            .collect(),
        docs_url: hint.docs_url.to_string(),
        repair_hint: hint.repair_hint.to_string(),
    }
}

fn work_bridge_repair_message(
    code: &str,
    operation: &str,
    owner: &str,
    repo: &str,
    issue_number: u64,
    work_key: Option<String>,
    reason: &str,
) -> WorkBridgeRepair {
    WorkBridgeRepair {
        code: code.to_string(),
        operation: operation.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        issue_number,
        work_key,
        reason: reason.to_string(),
        common_fixes: vec![
            "rerun the GitHub issue bridge after Work storage is healthy".to_string(),
            "inspect the Work item list for an existing mirrored issue".to_string(),
            "rerun cargo test -p jeryu-api --features web --jobs 40 github_issue_create_is_mirrored_into_work".to_string(),
        ],
        docs_url: "docs/testing.md#work".to_string(),
        repair_hint: "repair the Work mirror, then replay the issue bridge operation".to_string(),
    }
}

fn work_issue_url(issue: &Issue) -> String {
    format!(
        "/repos/jeryu/{}/{}/issues#{}",
        issue.owner, issue.repo, issue.number
    )
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
