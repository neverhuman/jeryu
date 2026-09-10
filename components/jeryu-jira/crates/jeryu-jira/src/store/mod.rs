use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::contracts::{
    CreateWorkCommentRequest, CreateWorkItemRequest, CreateWorkLinkRequest, UpdateWorkItemRequest,
    WorkComment, WorkFilter, WorkIssueLink, WorkItem, WorkItemDetail,
};
use crate::{Result, WorkError};

mod codec;
mod schema;
mod validation;

#[cfg(test)]
mod tests;

use codec::{parse_key, row_to_comment, row_to_item, row_to_item_result, to_json};
use validation::{
    filter_matches, local_operator, normalize_assignees, normalize_body, normalize_labels,
    validate_body, validate_issue_link, validate_principal, validate_pull_request_link,
    validate_title,
};

#[derive(Debug, Clone)]
pub struct WorkStore {
    path: PathBuf,
}

impl WorkStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(WorkError::storage)?;
        }
        let store = Self { path };
        let conn = store.connect()?;
        schema::apply(&conn)?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path).map_err(WorkError::storage)?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(WorkError::storage)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(WorkError::storage)?;
        Ok(conn)
    }

    pub fn create(&self, request: CreateWorkItemRequest) -> Result<WorkItem> {
        self.create_record(request, None)
    }

    fn create_record(
        &self,
        request: CreateWorkItemRequest,
        issue: Option<WorkIssueLink>,
    ) -> Result<WorkItem> {
        request.validate()?;
        let labels = normalize_labels(request.labels);
        let assignees = normalize_assignees(request.assignees)?;
        let now = Utc::now();
        let id = Uuid::new_v4();
        let repo = request.repo;
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(WorkError::storage)?;
        tx.execute(
            r#"
            INSERT INTO work_items (
                id, repo_id, repo_host, repo_owner, repo_name, title, body,
                status, kind, priority, labels_json, assignees_json,
                pull_requests_json, created_at, updated_at,
                issue_owner, issue_repo, issue_number, issue_url
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, '[]', ?13, ?13, ?14, ?15, ?16, ?17)
            "#,
            params![
                id.to_string(),
                repo.as_ref().map(|r| r.id.as_str()),
                repo.as_ref().map(|r| r.host.as_str()),
                repo.as_ref().map(|r| r.owner.as_str()),
                repo.as_ref().map(|r| r.name.as_str()),
                request.title.trim(),
                normalize_body(request.body),
                codec::encode_status(request.status.unwrap_or_default()),
                codec::encode_kind(request.kind.unwrap_or_default()),
                codec::encode_priority(request.priority.unwrap_or_default()),
                to_json(&labels)?,
                to_json(&assignees)?,
                now.to_rfc3339(),
                issue.as_ref().map(|link| link.owner.as_str()),
                issue.as_ref().map(|link| link.repo.as_str()),
                issue.as_ref().map(|link| link.number),
                issue.as_ref().and_then(|link| link.url.as_deref()),
            ],
        )
        .map_err(|error| match error {
            rusqlite::Error::SqliteFailure(code, _)
                if issue.is_some()
                    && code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                WorkError::Conflict("issue link already belongs to another work item".to_string())
            }
            error => WorkError::storage(error),
        })?;
        let number = u64::try_from(tx.last_insert_rowid())
            .map_err(|_| WorkError::Storage("work item number overflowed u64".to_string()))?;
        // Return only the row admitted by this transaction. A decoding failure
        // must not leave behind a committed creation that the caller could not read.
        let item = tx
            .query_row(
                "SELECT * FROM work_items WHERE number = ?1",
                params![number],
                row_to_item,
            )
            .map_err(WorkError::storage)?;
        tx.commit().map_err(WorkError::storage)?;
        Ok(item)
    }

    pub fn list(&self, filter: WorkFilter) -> Result<Vec<WorkItem>> {
        let conn = self.connect()?;
        let mut stmt = conn
            .prepare("SELECT * FROM work_items ORDER BY number ASC")
            .map_err(WorkError::storage)?;
        let mut rows = stmt.query([]).map_err(WorkError::storage)?;
        let mut items = Vec::new();
        while let Some(row) = rows.next().map_err(WorkError::storage)? {
            let item = row_to_item_result(row)?;
            if filter_matches(&item, &filter) {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn get(&self, key: &str) -> Result<WorkItem> {
        let number = parse_key(key)?;
        let conn = self.connect()?;
        conn.query_row(
            "SELECT * FROM work_items WHERE number = ?1",
            params![number],
            row_to_item,
        )
        .optional()
        .map_err(WorkError::storage)?
        .ok_or_else(|| WorkError::NotFound(key.to_string()))
    }

    pub fn detail(&self, key: &str) -> Result<WorkItemDetail> {
        let number = parse_key(key)?;
        let mut conn = self.connect()?;
        let tx = conn.transaction().map_err(WorkError::storage)?;
        let item = tx
            .query_row(
                "SELECT * FROM work_items WHERE number = ?1",
                params![number],
                row_to_item,
            )
            .optional()
            .map_err(WorkError::storage)?
            .ok_or_else(|| WorkError::NotFound(key.to_string()))?;
        #[cfg(test)]
        tests::after_detail_item_read();
        let comments = Self::comments_for_number(&tx, item.number)?;
        tx.commit().map_err(WorkError::storage)?;
        Ok(WorkItemDetail { item, comments })
    }

    pub fn patch(&self, key: &str, request: UpdateWorkItemRequest) -> Result<WorkItem> {
        let number = parse_key(key)?;
        let current = self.get(key)?;
        let title = match request.title {
            Some(title) => {
                validate_title(&title)?;
                title.trim().to_string()
            }
            None => current.title,
        };
        let body = request.body.or(current.body);
        let status = request.status.unwrap_or(current.status);
        let kind = request.kind.unwrap_or(current.kind);
        let priority = request.priority.unwrap_or(current.priority);
        let labels = request
            .labels
            .map(normalize_labels)
            .unwrap_or(current.labels);
        let assignees = match request.assignees {
            Some(assignees) => normalize_assignees(assignees)?,
            None => current.assignees,
        };
        let now = Utc::now();
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE work_items
            SET title = ?1,
                body = ?2,
                status = ?3,
                kind = ?4,
                priority = ?5,
                labels_json = ?6,
                assignees_json = ?7,
                updated_at = ?8
            WHERE number = ?9
            "#,
            params![
                title,
                normalize_body(body),
                codec::encode_status(status),
                codec::encode_kind(kind),
                codec::encode_priority(priority),
                to_json(&labels)?,
                to_json(&assignees)?,
                now.to_rfc3339(),
                number,
            ],
        )
        .map_err(WorkError::storage)?;
        self.get(key)
    }

    pub fn add_comment(&self, key: &str, request: CreateWorkCommentRequest) -> Result<WorkComment> {
        validate_body(&request.body, "comment body")?;
        let item = self.get(key)?;
        let author = request.author.unwrap_or_else(local_operator);
        validate_principal(&author)?;
        let now = Utc::now();
        let comment = WorkComment {
            id: Uuid::new_v4(),
            work_key: item.key,
            author,
            body: request.body.trim().to_string(),
            created_at: now,
        };
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO work_comments (id, work_number, author_json, body, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                comment.id.to_string(),
                item.number,
                to_json(&comment.author)?,
                comment.body,
                comment.created_at.to_rfc3339(),
            ],
        )
        .map_err(WorkError::storage)?;
        Ok(comment)
    }

    pub fn link(&self, key: &str, request: CreateWorkLinkRequest) -> Result<WorkItem> {
        if request.issue.is_none() && request.pull_request.is_none() {
            return Err(WorkError::Validation(
                "link request must include issue or pull_request".to_string(),
            ));
        }
        let mut item = self.get(key)?;
        let issue = request.issue.or(item.issue.clone());
        if let Some(issue) = &issue {
            validate_issue_link(issue)?;
        }
        let mut pull_requests = item.pull_requests;
        if let Some(pr) = request.pull_request {
            validate_pull_request_link(&pr)?;
            if !pull_requests.iter().any(|held| {
                held.owner == pr.owner && held.repo == pr.repo && held.number == pr.number
            }) {
                pull_requests.push(pr);
            }
        }
        let now = Utc::now();
        let conn = self.connect()?;
        let updated = conn.execute(
            r#"
            UPDATE work_items
            SET issue_owner = ?1,
                issue_repo = ?2,
                issue_number = ?3,
                issue_url = ?4,
                pull_requests_json = ?5,
                updated_at = ?6
            WHERE number = ?7
            "#,
            params![
                issue.as_ref().map(|i| i.owner.as_str()),
                issue.as_ref().map(|i| i.repo.as_str()),
                issue.as_ref().map(|i| i.number),
                issue.as_ref().and_then(|i| i.url.as_deref()),
                to_json(&pull_requests)?,
                now.to_rfc3339(),
                item.number,
            ],
        );
        match updated {
            Ok(_) => {
                item = self.get(key)?;
                Ok(item)
            }
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                Err(WorkError::Conflict(
                    "issue link already belongs to another work item".to_string(),
                ))
            }
            Err(error) => Err(WorkError::storage(error)),
        }
    }

    pub fn create_with_issue(
        &self,
        request: CreateWorkItemRequest,
        issue: WorkIssueLink,
    ) -> Result<WorkItem> {
        validate_issue_link(&issue)?;
        // Item and issue identity enter the Work store together or not at all.
        // The caller's Core store remains a separate authority.
        self.create_record(request, Some(issue))
    }

    pub fn find_by_issue(&self, owner: &str, repo: &str, number: u64) -> Result<Option<WorkItem>> {
        let conn = self.connect()?;
        conn.query_row(
            r#"
            SELECT * FROM work_items
            WHERE issue_owner = ?1 AND issue_repo = ?2 AND issue_number = ?3
            "#,
            params![owner, repo, number],
            row_to_item,
        )
        .optional()
        .map_err(WorkError::storage)
    }

    fn comments_for_number(conn: &Connection, number: u64) -> Result<Vec<WorkComment>> {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT id, author_json, body, created_at
                FROM work_comments
                WHERE work_number = ?1
                ORDER BY created_at ASC
                "#,
            )
            .map_err(WorkError::storage)?;
        let mut rows = stmt.query(params![number]).map_err(WorkError::storage)?;
        let mut comments = Vec::new();
        while let Some(row) = rows.next().map_err(WorkError::storage)? {
            comments.push(row_to_comment(row, number)?);
        }
        Ok(comments)
    }
}
