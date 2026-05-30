//! Owner: Web Forge BFF — MergeService (W-B-11).
//! Proof: `cargo nextest run -p jeryu --lib merge::service`
//! Invariants:
//!   - `approve_exact_sha` and `merge_exact_sha` ALWAYS refetch live state and
//!     compare the head SHA before writing (Tip1 Law 4; §28.3 supported-state pattern).
//!   - Every mutation writes audit + emits a WS event (`mr.approved` /
//!     `mr.merged`) per §35.1.14 steps 12-14.
//!   - The Phase-3 service is a thin pass-through over `GitHost`; local cache
//!     materialization (`web_merge_requests`, `passport_hash` row) lands with
//!     the §35.1.16 migration.

use std::sync::Arc;

use chrono::Utc;
use jeryu::api::entity::{ActionRef, EntityKind, EntityRef, HealthLevel};
use jeryu::api::merge_request::{
    AgentPosture, CheckPosture, MergePassport, MergePassportStatus, MergeRequestDetail,
    MergeRequestState, MergeRequestSummary, Mergeability, ReviewPosture,
};
use jeryu::api::repository::RepositoryId;
use jeryu::api::websocket::WebEvent;
use jeryu::git_host::{
    GitHost, GitLabClient, HostMergeInput, HostMergeResult, MrApproval, PrLiveState, PrSummary,
};
use jeryu::web_events::WebEventBus;
use serde_json::json;
use sqlx::AnyPool;

use crate::repos::models::RepoId;
use crate::repos::service::host_to_api_error;
use crate::web::audit::{RiskTier, write_audit};
use crate::web::error::ApiError;

use super::guards;
use super::merge_gate::MergePassportService;

/// Filter input for `MergeService::list`.
#[derive(Debug, Clone, Default)]
pub struct MergeListQuery {
    /// `"open"`, `"closed"`, `"merged"`, or `"all"`. Default is `"open"`.
    pub state: Option<String>,
}

/// Result of an approve/merge mutation (used for audit + event payload).
#[derive(Debug, Clone)]
pub struct MergeMutationResult {
    pub mr_iid: String,
    pub head_sha: String,
    pub receipt: String,
}

pub struct MergeService {
    host_name: String,
    gitlab: Arc<GitLabClient>,
    event_bus: Arc<WebEventBus>,
    passport: Arc<MergePassportService>,
    db_pool: AnyPool,
}

impl MergeService {
    pub fn new(
        host_name: impl Into<String>,
        gitlab: Arc<GitLabClient>,
        event_bus: Arc<WebEventBus>,
        passport: Arc<MergePassportService>,
        db_pool: AnyPool,
    ) -> Self {
        Self {
            host_name: host_name.into(),
            gitlab,
            event_bus,
            passport,
            db_pool,
        }
    }

    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    pub fn gitlab_client(&self) -> Arc<GitLabClient> {
        self.gitlab.clone()
    }

    /// `GET /api/v1/repos/{repo_id}/merge-requests?state=`. Phase 3:
    /// pass-through over `host.list_open_prs`; `state` filter is applied
    /// client-side (host only exposes `state=opened|closed|merged|all`).
    pub async fn list(
        &self,
        repo_id: &str,
        query: MergeListQuery,
    ) -> Result<Vec<MergeRequestSummary>, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        // Phase 3: host gives us only open PRs through `list_open_prs`. For
        // `state=open` (default) that's exactly right; other states would
        // require an extension to the host adapter and surface as "no
        // results" today.
        let want_open = matches!(
            query.state.as_deref().unwrap_or("open"),
            "open" | "opened" | "all"
        );
        let summaries = if want_open {
            let prs: Vec<PrSummary> = GitHost::list_open_prs(self.gitlab.as_ref(), &repo)
                .await
                .map_err(host_to_api_error)?;
            let repo_dto = RepositoryId::from(&parsed);
            prs.into_iter()
                .map(|p| summary_from_pr(&repo_dto, &p))
                .collect()
        } else {
            Vec::new()
        };
        Ok(summaries)
    }

    /// `GET /api/v1/repos/{repo_id}/merge-requests/{iid}`. Includes the live
    /// `MergePassport` verdict.
    pub async fn get(&self, repo_id: &str, iid: &str) -> Result<MergeRequestDetail, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let live = GitHost::get_pr_state(self.gitlab.as_ref(), &repo, iid)
            .await
            .map_err(host_to_api_error)?;
        let repo_dto = RepositoryId::from(&parsed);
        let summary = summary_from_live(&repo_dto, iid, &live);
        let passport = self.passport.compute(&parsed, iid).await?;
        Ok(MergeRequestDetail {
            summary,
            description: None,
            passport_hash: passport_hash(&passport),
            merge_passport: passport,
        })
    }

    /// `GET /api/v1/repos/{repo_id}/merge-requests/{iid}/diff` — returns the
    /// canonical per-file diff via the host adapter.
    pub async fn diff(
        &self,
        repo_id: &str,
        iid: &str,
    ) -> Result<jeryu::git_host::PrDiff, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        GitHost::fetch_pr_diff(self.gitlab.as_ref(), &repo, iid)
            .await
            .map_err(host_to_api_error)
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/approve` — §28.3
    /// exact-SHA approve guard + audit + event.
    pub async fn approve_exact_sha(
        &self,
        repo_id: &str,
        iid: &str,
        expected_head_sha: &str,
        actor: &str,
        idempotency_key: Option<&str>,
    ) -> Result<MergeMutationResult, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        let bound =
            guards::verify_head_sha(self.gitlab.as_ref(), &repo, iid, expected_head_sha).await?;
        let receipt_digest = compute_receipt(iid, &bound);
        let receipt = GitHost::approve_mr(
            self.gitlab.as_ref(),
            MrApproval {
                repo: &repo,
                mr_iid: iid,
                head_sha: &bound,
                agent_id: actor,
                receipt_digest: &receipt_digest,
                dry_run: false,
            },
        )
        .await
        .map_err(host_to_api_error)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.approve",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::High,
            json!({
                "iid": iid,
                "head_sha": bound,
                "receipt": receipt,
                "idempotency_key": idempotency_key,
            }),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.approve)"
            );
        }
        self.publish_event(
            &parsed,
            iid,
            "mr.approved",
            "merge request approved",
            json!({"iid": iid, "head_sha": bound, "actor": actor, "receipt": receipt}),
        );
        Ok(MergeMutationResult {
            mr_iid: iid.to_string(),
            head_sha: bound,
            receipt,
        })
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/merge` — verify
    /// SHA fence + verify Merge Passport pass + write merge.
    #[allow(clippy::too_many_arguments)]
    pub async fn merge_exact_sha(
        &self,
        repo_id: &str,
        iid: &str,
        expected_head_sha: &str,
        method: &str,
        commit_title: Option<&str>,
        commit_message: Option<&str>,
        actor: &str,
        idempotency_key: Option<&str>,
    ) -> Result<MergeMutationResult, ApiError> {
        let parsed = parse_repo_id(repo_id)?;
        let repo = parsed.repo_ref();
        // 1. SHA fence
        let bound =
            guards::verify_head_sha(self.gitlab.as_ref(), &repo, iid, expected_head_sha).await?;
        // 2. Passport must pass
        let passport = self.passport.compute(&parsed, iid).await?;
        if matches!(passport.status, MergePassportStatus::Blocked) {
            let blockers: Vec<String> = passport.blockers.iter().map(|b| b.code.clone()).collect();
            return Err(ApiError::Conflict(format!(
                "merge_passport_blocked: {}",
                blockers.join(", ")
            )));
        }
        // 3. Merge via host
        let result = GitHost::merge_mr(
            self.gitlab.as_ref(),
            HostMergeInput {
                repo: &repo,
                mr_iid: iid,
                expected_head_sha: &bound,
                method,
                commit_title,
                commit_message,
            },
        )
        .await
        .map_err(host_to_api_error)?;
        let receipt = match merge_receipt_from_result(&result) {
            Ok(receipt) => receipt,
            Err(err) => {
                tracing::warn!(
                    target: "jeryu.web.audit",
                    "merge result missing receipt sha: {}",
                    err
                );
                return Err(err);
            }
        };
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.merge",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::High,
            json!({
                "iid": iid,
                "head_sha": bound,
                "method": method,
                "merged": result.merged,
                "result_sha": result.sha,
                "idempotency_key": idempotency_key,
            }),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.merge)"
            );
        }
        self.publish_event(
            &parsed,
            iid,
            "mr.merged",
            "merge request merged",
            json!({
                "iid": iid,
                "head_sha": bound,
                "method": method,
                "result_sha": result.sha,
                "merged": result.merged,
            }),
        );
        Ok(MergeMutationResult {
            mr_iid: iid.to_string(),
            head_sha: bound,
            receipt,
        })
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/close` — unavailable
    /// until the GitLab host adapter supports this mutation. GitLab adapter
    /// doesn't expose a "close" call yet; surface a typed upstream error so
    /// the UI can degrade gracefully without breaking the API contract.
    pub async fn close_mr(&self, repo_id: &str, iid: &str, actor: &str) -> Result<(), ApiError> {
        let _parsed = parse_repo_id(repo_id)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.close",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::Medium,
            json!({"iid": iid}),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.close)"
            );
        }
        Err(ApiError::Upstream(
            "mr.close requires GitLab host adapter support".into(),
        ))
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/reopen` — unavailable
    /// until the GitLab host adapter supports this mutation. See `close_mr`.
    pub async fn reopen_mr(&self, repo_id: &str, iid: &str, actor: &str) -> Result<(), ApiError> {
        let _parsed = parse_repo_id(repo_id)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.reopen",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::Medium,
            json!({"iid": iid}),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.reopen)"
            );
        }
        Err(ApiError::Upstream(
            "mr.reopen requires GitLab host adapter support".into(),
        ))
    }

    /// `POST /api/v1/repos/{repo_id}/merge-requests/{iid}/rebase` — unavailable
    /// until the GitLab host adapter supports this mutation. See `close_mr`.
    pub async fn rebase_mr(&self, repo_id: &str, iid: &str, actor: &str) -> Result<(), ApiError> {
        let _parsed = parse_repo_id(repo_id)?;
        if let Err(err) = write_audit(
            &self.db_pool,
            actor,
            "mr.rebase",
            &format!("mr:{repo_id}/{iid}"),
            RiskTier::Medium,
            json!({"iid": iid}),
        )
        .await
        {
            tracing::warn!(
                target: "jeryu.web.audit",
                error = %err,
                "audit event write failed (mr.rebase)"
            );
        }
        Err(ApiError::Upstream(
            "mr.rebase requires GitLab host adapter support".into(),
        ))
    }

    /// Helper: publish an MR-scoped WebEvent on the bus.
    fn publish_event(
        &self,
        repo: &RepoId,
        iid: &str,
        kind: &str,
        summary: &str,
        payload: serde_json::Value,
    ) {
        let scope = format!("mr.{repo}/{iid}", repo = repo.encode());
        let entity = format!("mr:{repo}/{iid}", repo = repo.encode());
        let evt = WebEvent {
            seq: 0,
            timestamp: Utc::now(),
            scope,
            kind: kind.to_string(),
            entity,
            summary: summary.to_string(),
            payload,
        };
        let _ = self.event_bus.publish(evt);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn parse_repo_id(raw: &str) -> Result<RepoId, ApiError> {
    match RepoId::parse(raw) {
        Some(repo_id) => Ok(repo_id),
        None => Err(ApiError::BadRequest(format!("invalid repo_id: {raw}"))),
    }
}

fn merge_receipt_from_result(result: &HostMergeResult) -> Result<String, ApiError> {
    match result.sha.as_ref() {
        Some(receipt) => Ok(receipt.clone()),
        None => Err(ApiError::Upstream(
            "merge completed without a host receipt sha".into(),
        )),
    }
}

/// Stable receipt digest used for `MrApproval::receipt_digest` and the audit
/// row. Phase 3: deterministic SHA over `(iid, head_sha)`; future phases bind
/// to the evidence pack digest emitted by the autonomy daemon.
fn compute_receipt(iid: &str, head_sha: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"jeryu:mr.approve:");
    h.update(iid.as_bytes());
    h.update(b":");
    h.update(head_sha.as_bytes());
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Compute a stable `passport_hash` over the Merge Passport's blockers +
/// status + head_sha. Used by §35.1.14 step 12 (`expected_state_hash`).
fn passport_hash(passport: &MergePassport) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(passport.head_sha.as_bytes());
    h.update(b"|");
    h.update(
        match passport.status {
            MergePassportStatus::Pass => "pass",
            MergePassportStatus::Blocked => "blocked",
        }
        .as_bytes(),
    );
    for b in &passport.blockers {
        h.update(b"|");
        h.update(b.code.as_bytes());
    }
    Some(format!("sha256:{}", hex::encode(h.finalize())))
}

/// Build a `MergeRequestSummary` from a list-page `PrSummary`. The Phase-3
/// summary uses neutral posture counters; W-B-12/13 fill review/checks/
/// agent posture from the host adapter.
fn summary_from_pr(repo: &RepositoryId, pr: &PrSummary) -> MergeRequestSummary {
    MergeRequestSummary {
        repo: repo.clone(),
        iid: pr.mr_iid.clone(),
        entity: EntityRef::new(EntityKind::MergeRequest, pr.mr_iid.clone()),
        title: pr.title.clone(),
        author: pr.author.clone(),
        source_branch: String::new(),
        target_branch: pr.target_branch.clone(),
        head_sha: pr.head_sha.clone(),
        base_sha: String::new(),
        state: MergeRequestState::Open,
        draft: pr.draft,
        mergeable: Mergeability {
            level: HealthLevel::Unknown,
            can_merge: false,
            reason: None,
            exact_head_sha: pr.head_sha.clone(),
            required_gate: Some("merge_passport".into()),
        },
        review: ReviewPosture {
            required_approvals: 1,
            approvals: 0,
            changes_requested: 0,
            unresolved_threads: 0,
            user_review_state: None,
        },
        checks: CheckPosture {
            total: 0,
            passing: 0,
            failing: 0,
            pending: 0,
            skipped: 0,
        },
        agents: AgentPosture {
            active_sessions: 0,
            proposed_patches: 0,
            evidence_packets: 0,
            blockers: 0,
        },
        labels: pr.labels.clone(),
        updated_at: Utc::now(),
        passport_hash: None,
        available_actions: vec![
            ActionRef {
                action_id: "mr.approve".into(),
                label: "Approve".into(),
                risk: None,
            },
            ActionRef {
                action_id: "mr.merge".into(),
                label: "Merge".into(),
                risk: None,
            },
        ],
    }
}

/// Build a `MergeRequestSummary` from a live `PrLiveState` (single MR fetch).
fn summary_from_live(repo: &RepositoryId, iid: &str, live: &PrLiveState) -> MergeRequestSummary {
    MergeRequestSummary {
        repo: repo.clone(),
        iid: iid.to_string(),
        entity: EntityRef::new(EntityKind::MergeRequest, iid.to_string()),
        title: format!("MR !{}", iid),
        author: "unknown".into(),
        source_branch: String::new(),
        target_branch: live.target_branch.clone(),
        head_sha: live.head_sha.clone(),
        base_sha: live.target_branch_sha.clone(),
        state: MergeRequestState::Open,
        draft: false,
        mergeable: Mergeability {
            level: HealthLevel::Unknown,
            can_merge: false,
            reason: None,
            exact_head_sha: live.head_sha.clone(),
            required_gate: Some("merge_passport".into()),
        },
        review: ReviewPosture {
            required_approvals: 1,
            approvals: 0,
            changes_requested: 0,
            unresolved_threads: 0,
            user_review_state: None,
        },
        checks: CheckPosture {
            total: 0,
            passing: 0,
            failing: 0,
            pending: 0,
            skipped: 0,
        },
        agents: AgentPosture {
            active_sessions: 0,
            proposed_patches: 0,
            evidence_packets: 0,
            blockers: 0,
        },
        labels: vec![],
        updated_at: live.fetched_at,
        passport_hash: None,
        available_actions: vec![
            ActionRef {
                action_id: "mr.approve".into(),
                label: "Approve".into(),
                risk: None,
            },
            ActionRef {
                action_id: "mr.merge".into(),
                label: "Merge".into(),
                risk: None,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu::api::merge_request::{MergePassport, MergePassportBlocker, MergePassportStatus};

    #[test]
    fn compute_receipt_is_deterministic() {
        let a = compute_receipt("42", "abc");
        let b = compute_receipt("42", "abc");
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn compute_receipt_differs_on_sha_change() {
        assert_ne!(compute_receipt("42", "aaa"), compute_receipt("42", "bbb"));
    }

    #[test]
    fn merge_receipt_from_result_requires_sha() {
        let result = HostMergeResult {
            merged: true,
            sha: Some("abc".into()),
            url: None,
        };
        assert_eq!(merge_receipt_from_result(&result).unwrap(), "abc");

        let missing = HostMergeResult {
            merged: true,
            sha: None,
            url: None,
        };
        assert!(matches!(
            merge_receipt_from_result(&missing),
            Err(ApiError::Upstream(message)) if message.contains("host receipt sha")
        ));
    }

    #[test]
    fn passport_hash_is_stable() {
        let p = MergePassport {
            status: MergePassportStatus::Blocked,
            head_sha: "abc".into(),
            blockers: vec![MergePassportBlocker {
                code: "passport_blocked_approvals".into(),
                message: "x".into(),
                details: None,
            }],
            evaluated_at: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap(),
        };
        let h1 = passport_hash(&p).unwrap();
        let h2 = passport_hash(&p).unwrap();
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }

    #[test]
    fn merge_list_query_defaults_to_open() {
        let q = MergeListQuery::default();
        assert!(q.state.is_none());
    }

    #[test]
    fn summary_from_pr_carries_labels_and_actions() {
        let repo = RepositoryId {
            id: "gitlab/own/proj".into(),
            host: "gitlab".into(),
            owner: "own".into(),
            name: "proj".into(),
        };
        let pr = PrSummary {
            mr_iid: "7".into(),
            head_sha: "deadbeef".into(),
            target_branch: "main".into(),
            author: "alice".into(),
            title: "fix bug".into(),
            draft: false,
            labels: vec!["bug".into()],
        };
        let s = summary_from_pr(&repo, &pr);
        assert_eq!(s.iid, "7");
        assert_eq!(s.labels.len(), 1);
        assert_eq!(s.available_actions.len(), 2);
    }
}
