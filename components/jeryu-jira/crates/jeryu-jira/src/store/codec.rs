use chrono::{DateTime, Utc};
use rusqlite::Row;
use uuid::Uuid;

use crate::contracts::{
    WorkComment, WorkIssueLink, WorkItem, WorkItemKind, WorkPriority, WorkRepository, WorkStatus,
};
use crate::{Result, WorkError};

pub(super) fn row_to_item(row: &Row<'_>) -> rusqlite::Result<WorkItem> {
    row_to_item_result(row).map_err(|error| match error {
        WorkError::Storage(message) | WorkError::Serialization(message) => {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(WorkError::Serialization(message)),
            )
        }
        other => rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(other),
        ),
    })
}

pub(super) fn row_to_item_result(row: &Row<'_>) -> Result<WorkItem> {
    let number_i64: i64 = row.get("number").map_err(WorkError::storage)?;
    let number = positive_u64(number_i64, "work item number")?;
    let id: String = row.get("id").map_err(WorkError::storage)?;
    let labels_json: String = row.get("labels_json").map_err(WorkError::storage)?;
    let assignees_json: String = row.get("assignees_json").map_err(WorkError::storage)?;
    let pull_requests_json: String = row.get("pull_requests_json").map_err(WorkError::storage)?;
    let repo_id: Option<String> = row.get("repo_id").map_err(WorkError::storage)?;
    let repo = match repo_id {
        Some(id) => Some(WorkRepository {
            id,
            host: row
                .get::<_, Option<String>>("repo_host")
                .map_err(WorkError::storage)?
                .unwrap_or_else(|| "jeryu".to_string()),
            owner: row
                .get::<_, Option<String>>("repo_owner")
                .map_err(WorkError::storage)?
                .unwrap_or_default(),
            name: row
                .get::<_, Option<String>>("repo_name")
                .map_err(WorkError::storage)?
                .unwrap_or_default(),
        }),
        None => None,
    };
    let issue_owner: Option<String> = row.get("issue_owner").map_err(WorkError::storage)?;
    let issue_repo: Option<String> = row.get("issue_repo").map_err(WorkError::storage)?;
    let issue_number: Option<i64> = row.get("issue_number").map_err(WorkError::storage)?;
    let issue = match (issue_owner, issue_repo, issue_number) {
        (Some(owner), Some(repo), Some(number)) => Some(WorkIssueLink {
            owner,
            repo,
            number: positive_u64(number, "issue number")?,
            url: row.get("issue_url").map_err(WorkError::storage)?,
        }),
        (None, None, None) => None,
        _ => {
            return Err(WorkError::Serialization(
                "incomplete work issue link row".to_string(),
            ));
        }
    };
    let status_text: String = row.get("status").map_err(WorkError::storage)?;
    let kind_text: String = row.get("kind").map_err(WorkError::storage)?;
    let priority_text: String = row.get("priority").map_err(WorkError::storage)?;
    let created_at: String = row.get("created_at").map_err(WorkError::storage)?;
    let updated_at: String = row.get("updated_at").map_err(WorkError::storage)?;
    Ok(WorkItem {
        id: Uuid::parse_str(&id).map_err(WorkError::serialization)?,
        key: key_for(number),
        number,
        repo,
        title: row.get("title").map_err(WorkError::storage)?,
        body: row.get("body").map_err(WorkError::storage)?,
        status: decode_status(&status_text)?,
        kind: decode_kind(&kind_text)?,
        priority: decode_priority(&priority_text)?,
        labels: from_json(&labels_json)?,
        assignees: from_json(&assignees_json)?,
        issue,
        pull_requests: from_json(&pull_requests_json)?,
        created_at: parse_time(&created_at)?,
        updated_at: parse_time(&updated_at)?,
    })
}

pub(super) fn row_to_comment(row: &Row<'_>, number: u64) -> Result<WorkComment> {
    let id: String = row.get("id").map_err(WorkError::storage)?;
    let author_json: String = row.get("author_json").map_err(WorkError::storage)?;
    let body: String = row.get("body").map_err(WorkError::storage)?;
    let created_at: String = row.get("created_at").map_err(WorkError::storage)?;
    Ok(WorkComment {
        id: Uuid::parse_str(&id).map_err(WorkError::serialization)?,
        work_key: key_for(number),
        author: from_json(&author_json)?,
        body,
        created_at: parse_time(&created_at)?,
    })
}

pub(super) fn key_for(number: u64) -> String {
    format!("JRY-{number}")
}

pub(super) fn parse_key(key: &str) -> Result<u64> {
    let raw = key
        .strip_prefix("JRY-")
        .or_else(|| key.strip_prefix("jry-"))
        .ok_or_else(|| WorkError::Validation("work key must look like JRY-1".to_string()))?;
    let number = raw
        .parse::<u64>()
        .map_err(|_| WorkError::Validation("work key must include a numeric suffix".to_string()))?;
    if number == 0 {
        return Err(WorkError::Validation(
            "work key number must be greater than zero".to_string(),
        ));
    }
    Ok(number)
}

pub(super) fn encode_status(status: WorkStatus) -> &'static str {
    match status {
        WorkStatus::Backlog => "backlog",
        WorkStatus::Ready => "ready",
        WorkStatus::InProgress => "in_progress",
        WorkStatus::Blocked => "blocked",
        WorkStatus::InReview => "in_review",
        WorkStatus::Done => "done",
        WorkStatus::Canceled => "canceled",
    }
}

fn decode_status(value: &str) -> Result<WorkStatus> {
    match value {
        "backlog" => Ok(WorkStatus::Backlog),
        "ready" => Ok(WorkStatus::Ready),
        "in_progress" => Ok(WorkStatus::InProgress),
        "blocked" => Ok(WorkStatus::Blocked),
        "in_review" => Ok(WorkStatus::InReview),
        "done" => Ok(WorkStatus::Done),
        "canceled" => Ok(WorkStatus::Canceled),
        other => Err(WorkError::Serialization(format!(
            "unknown work status {other}"
        ))),
    }
}

pub(super) fn encode_kind(kind: WorkItemKind) -> &'static str {
    match kind {
        WorkItemKind::Task => "task",
        WorkItemKind::Bug => "bug",
        WorkItemKind::Chore => "chore",
        WorkItemKind::Docs => "docs",
        WorkItemKind::Ci => "ci",
    }
}

fn decode_kind(value: &str) -> Result<WorkItemKind> {
    match value {
        "task" => Ok(WorkItemKind::Task),
        "bug" => Ok(WorkItemKind::Bug),
        "chore" => Ok(WorkItemKind::Chore),
        "docs" => Ok(WorkItemKind::Docs),
        "ci" => Ok(WorkItemKind::Ci),
        other => Err(WorkError::Serialization(format!(
            "unknown work kind {other}"
        ))),
    }
}

pub(super) fn encode_priority(priority: WorkPriority) -> &'static str {
    match priority {
        WorkPriority::P0 => "p0",
        WorkPriority::P1 => "p1",
        WorkPriority::P2 => "p2",
        WorkPriority::P3 => "p3",
        WorkPriority::P4 => "p4",
    }
}

fn decode_priority(value: &str) -> Result<WorkPriority> {
    match value {
        "p0" => Ok(WorkPriority::P0),
        "p1" => Ok(WorkPriority::P1),
        "p2" => Ok(WorkPriority::P2),
        "p3" => Ok(WorkPriority::P3),
        "p4" => Ok(WorkPriority::P4),
        other => Err(WorkError::Serialization(format!(
            "unknown work priority {other}"
        ))),
    }
}

pub(super) fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(WorkError::serialization)
}

fn from_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(WorkError::serialization)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(WorkError::serialization)
}

fn positive_u64(value: i64, field: &str) -> Result<u64> {
    let number =
        u64::try_from(value).map_err(|_| WorkError::Serialization(format!("negative {field}")))?;
    if number == 0 {
        return Err(WorkError::Serialization(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(number)
}
