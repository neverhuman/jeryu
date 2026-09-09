use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::{Config, ExportError, TS};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    #[default]
    Task,
    Bug,
    Chore,
    Docs,
    Ci,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkStatus {
    #[default]
    Backlog,
    Ready,
    InProgress,
    Blocked,
    InReview,
    Done,
    Canceled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkPriority {
    P0,
    P1,
    #[default]
    P2,
    P3,
    P4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkPrincipalKind {
    Human,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkPrincipal {
    pub kind: WorkPrincipalKind,
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkRepository {
    pub id: String,
    pub host: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkIssueLink {
    pub owner: String,
    pub repo: String,
    #[ts(type = "number")]
    pub number: u64,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkPullRequestLink {
    pub owner: String,
    pub repo: String,
    #[ts(type = "number")]
    pub number: u64,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkItem {
    pub id: Uuid,
    pub key: String,
    #[ts(type = "number")]
    pub number: u64,
    #[serde(default)]
    pub repo: Option<WorkRepository>,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub status: WorkStatus,
    pub kind: WorkItemKind,
    pub priority: WorkPriority,
    pub labels: Vec<String>,
    pub assignees: Vec<WorkPrincipal>,
    #[serde(default)]
    pub issue: Option<WorkIssueLink>,
    pub pull_requests: Vec<WorkPullRequestLink>,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "string")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkComment {
    pub id: Uuid,
    pub work_key: String,
    pub author: WorkPrincipal,
    pub body: String,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkItemDetail {
    pub item: WorkItem,
    pub comments: Vec<WorkComment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkItemListResponse {
    pub items: Vec<WorkItem>,
    pub total: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateWorkItemRequest {
    #[serde(default)]
    pub repo: Option<WorkRepository>,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub status: Option<WorkStatus>,
    #[serde(default)]
    pub kind: Option<WorkItemKind>,
    #[serde(default)]
    pub priority: Option<WorkPriority>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignees: Vec<WorkPrincipal>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateWorkItemRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub status: Option<WorkStatus>,
    #[serde(default)]
    pub kind: Option<WorkItemKind>,
    #[serde(default)]
    pub priority: Option<WorkPriority>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    #[serde(default)]
    pub assignees: Option<Vec<WorkPrincipal>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateWorkCommentRequest {
    pub body: String,
    #[serde(default)]
    pub author: Option<WorkPrincipal>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateWorkLinkRequest {
    #[serde(default)]
    pub issue: Option<WorkIssueLink>,
    #[serde(default)]
    pub pull_request: Option<WorkPullRequestLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkFilter {
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub status: Option<WorkStatus>,
    #[serde(default)]
    pub kind: Option<WorkItemKind>,
    #[serde(default)]
    pub priority: Option<WorkPriority>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}

macro_rules! contract_exports {
    ($($ty:ty),+ $(,)?) => {
        pub const CONTRACT_COUNT: usize = { let mut n = 0; $( n += 1; let _ = stringify!($ty); )+ n };

        pub fn export_all_contracts(cfg: &Config) -> std::result::Result<(), ExportError> {
            $( <$ty as TS>::export(cfg)?; )+
            Ok(())
        }

        pub fn contract_files() -> Vec<(&'static str, String)> {
            let mut out = Vec::new();
            $(
                let path = <$ty as TS>::output_path().expect("contract type exports");
                out.push((stringify!($ty), path.to_string_lossy().into_owned()));
            )+
            out
        }
    };
}

contract_exports!(
    CreateWorkCommentRequest,
    CreateWorkItemRequest,
    CreateWorkLinkRequest,
    UpdateWorkItemRequest,
    WorkComment,
    WorkFilter,
    WorkIssueLink,
    WorkItem,
    WorkItemDetail,
    WorkItemKind,
    WorkItemListResponse,
    WorkPrincipal,
    WorkPrincipalKind,
    WorkPriority,
    WorkPullRequestLink,
    WorkRepository,
    WorkStatus,
);
