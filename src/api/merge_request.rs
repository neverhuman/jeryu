//! Owner: TUI Control-Plane API + Web Forge BFF — merge request DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.2 + §35.1.14 (`passport_hash`) and §35.2.4
//! (canonical 12-gate Passport check list).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct MergeRequestSummary {
    pub repo: RepositoryId,
    pub iid: String,
    #[cfg_attr(feature = "web", ts(type = "{ kind: string; id: string }"))]
    #[cfg_attr(
        feature = "web",
        schemars(with = "super::repository::EntityRefSurrogate")
    )]
    #[cfg_attr(feature = "web", schema(value_type = super::repository::EntityRefSurrogate))]
    pub entity: super::entity::EntityRef,
    pub title: String,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub state: MergeRequestState,
    pub draft: bool,
    pub mergeable: Mergeability,
    pub review: ReviewPosture,
    pub checks: CheckPosture,
    pub agents: AgentPosture,
    pub labels: Vec<String>,
    pub updated_at: DateTime<Utc>,
    /// Stable hash of the most recent Merge Passport evaluation. Per §35.1.14
    /// the backend persists `passport_hash` on `web_merge_requests` after each
    /// Passport recompute so we can detect re-evaluation drift across replays.
    /// `None` when the Passport has not been computed yet (e.g. brand-new MR).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passport_hash: Option<String>,
    #[cfg_attr(
        feature = "web",
        ts(type = "Array<{ action_id: string; label: string; risk: string | null }>")
    )]
    #[cfg_attr(
        feature = "web",
        schemars(with = "Vec<super::repository::ActionRefSurrogate>")
    )]
    #[cfg_attr(feature = "web", schema(value_type = Vec<super::repository::ActionRefSurrogate>))]
    pub available_actions: Vec<super::entity::ActionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub enum MergeRequestState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct Mergeability {
    #[cfg_attr(feature = "web", ts(type = "string"))]
    #[cfg_attr(feature = "web", schemars(with = "String"))]
    #[cfg_attr(feature = "web", schema(value_type = String))]
    pub level: super::entity::HealthLevel,
    pub can_merge: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub exact_head_sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_gate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct ReviewPosture {
    pub required_approvals: u32,
    pub approvals: u32,
    pub changes_requested: u32,
    pub unresolved_threads: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_review_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct CheckPosture {
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    pub skipped: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct AgentPosture {
    pub active_sessions: u32,
    pub proposed_patches: u32,
    pub evidence_packets: u32,
    pub blockers: u32,
}

/// Full MR view returned by `GET /api/v1/repos/{repo_id}/merge-requests/{iid}`.
///
/// Carries the same posture summary fields plus the Merge Passport verdict
/// (per §35.2.4) so the UI's `MergeGatePanel` can render gate-by-gate detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct MergeRequestDetail {
    pub summary: MergeRequestSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub merge_passport: MergePassport,
    /// `passport_hash` mirrors `summary.passport_hash` for ergonomics so the
    /// UI doesn't need to dive into `summary` to display the verdict identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passport_hash: Option<String>,
}

/// Canonical Merge Passport verdict (FINAL spec §6.7 + §35.2.4 12-gate list).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct MergePassport {
    pub status: MergePassportStatus,
    pub head_sha: String,
    pub blockers: Vec<MergePassportBlocker>,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub enum MergePassportStatus {
    Pass,
    Blocked,
}

/// One blocker entry in the Merge Passport. `code` aligns with the §35.2.4
/// canonical gate list (e.g. `passport_blocked_approvals`,
/// `passport_blocked_policy_sha`) so the UI can target explanations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(
    feature = "web",
    derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)
)]
#[cfg_attr(feature = "web", ts(export, export_to = "../../contracts/generated/"))]
pub struct MergePassportBlocker {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::entity::{ActionRef, EntityKind, EntityRef, HealthLevel};

    fn sample_summary() -> MergeRequestSummary {
        MergeRequestSummary {
            repo: super::super::repository::RepositoryId {
                id: "11111111-2222-3333-4444-555555555555".into(),
                host: "gitlab".into(),
                owner: "neverhuman".into(),
                name: "jeryu".into(),
            },
            iid: "42".into(),
            entity: EntityRef::new(EntityKind::MergeRequest, "42"),
            title: "feat: add typed DTOs".into(),
            author: "claude".into(),
            source_branch: "web-forge/W-F-03-dtos".into(),
            target_branch: "web-forge/main".into(),
            head_sha: "abc123".into(),
            base_sha: "def456".into(),
            state: MergeRequestState::Open,
            draft: false,
            mergeable: Mergeability {
                level: HealthLevel::Warning,
                can_merge: false,
                reason: Some("required approvals not met".into()),
                exact_head_sha: "abc123".into(),
                required_gate: Some("approvals".into()),
            },
            review: ReviewPosture {
                required_approvals: 2,
                approvals: 1,
                changes_requested: 0,
                unresolved_threads: 1,
                user_review_state: Some("approved".into()),
            },
            checks: CheckPosture {
                total: 8,
                passing: 7,
                failing: 0,
                pending: 1,
                skipped: 0,
            },
            agents: AgentPosture {
                active_sessions: 0,
                proposed_patches: 0,
                evidence_packets: 0,
                blockers: 0,
            },
            labels: vec!["needs-review".into()],
            updated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            passport_hash: Some("sha256:deadbeef".into()),
            available_actions: vec![ActionRef {
                action_id: "mr.approve".into(),
                label: "Approve".into(),
                risk: None,
            }],
        }
    }

    #[test]
    fn merge_request_summary_roundtrips_with_passport_hash() {
        let s = sample_summary();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("passport_hash"));
        let back: MergeRequestSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.iid, "42");
        assert_eq!(back.passport_hash.as_deref(), Some("sha256:deadbeef"));
        assert!(matches!(back.state, MergeRequestState::Open));
    }

    #[test]
    fn merge_request_summary_omits_passport_hash_when_absent() {
        let mut s = sample_summary();
        s.passport_hash = None;
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("passport_hash"));
    }

    #[test]
    fn merge_request_state_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&MergeRequestState::Merged).unwrap(),
            "\"merged\""
        );
    }

    #[test]
    fn passport_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&MergePassportStatus::Blocked).unwrap(),
            "\"blocked\""
        );
    }

    #[test]
    fn merge_request_detail_roundtrips() {
        let detail = MergeRequestDetail {
            summary: sample_summary(),
            description: Some("body".into()),
            merge_passport: MergePassport {
                status: MergePassportStatus::Blocked,
                head_sha: "abc123".into(),
                blockers: vec![MergePassportBlocker {
                    code: "passport_blocked_approvals".into(),
                    message: "Required approvals not met (1/2)".into(),
                    details: Some("missing CODEOWNERS approval".into()),
                }],
                evaluated_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
            },
            passport_hash: Some("sha256:deadbeef".into()),
        };
        let json = serde_json::to_string(&detail).unwrap();
        let back: MergeRequestDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(back.merge_passport.blockers.len(), 1);
        assert!(matches!(
            back.merge_passport.status,
            MergePassportStatus::Blocked
        ));
        assert_eq!(back.passport_hash.as_deref(), Some("sha256:deadbeef"));
    }
}
