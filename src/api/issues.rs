//! Placeholder DTOs for the Issues feature (target v1.5).
//!
//! Stubs exist so frontend code can compile against typed shapes; backend
//! handlers return 501 until W-B-30 lands.
//!
//! Source: WEB_WORK_CLAUDE.md §7.0 W-F-03 + §35.7 (issues stubs return 501).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct IssueSummary {
    pub repo: RepositoryId,
    pub iid: String,
    pub title: String,
    pub state: IssueState,
    pub labels: Vec<String>,
    pub author: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum IssueState {
    Open,
    Closed,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_state_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&IssueState::Open).unwrap(), "\"open\"");
        assert_eq!(
            serde_json::to_string(&IssueState::Closed).unwrap(),
            "\"closed\""
        );
    }

    #[test]
    fn issue_summary_roundtrips() {
        let s = IssueSummary {
            repo: RepositoryId {
                id: "repo-1".into(),
                host: "gitlab".into(),
                owner: "neverhuman".into(),
                name: "jeryu".into(),
            },
            iid: "7".into(),
            title: "stub".into(),
            state: IssueState::Open,
            labels: vec!["v1.5".into()],
            author: "claude".into(),
            created_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            updated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: IssueSummary = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.state, IssueState::Open));
        assert_eq!(back.labels, vec!["v1.5".to_string()]);
    }
}
