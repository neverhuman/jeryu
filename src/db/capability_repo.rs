//! Capability persistence and state read boundary.
//!
//! Capability transports call this typed service instead of opening state
//! directly, keeping DB ownership under `src/db/`.

use anyhow::Result;

use crate::capsule::FailureCapsule;
use crate::state::{CiJobBottleneck, JobEvent, Pool, ReleaseAttempt};

#[derive(Clone)]
pub struct CapabilityRepo {
    db: crate::state::Db,
}

#[derive(Debug, Clone)]
pub struct CapabilitySystemSnapshot {
    pub pools: Vec<Pool>,
    pub recent_jobs: Vec<JobEvent>,
    pub latest_release: Option<ReleaseAttempt>,
}

pub struct BranchCapabilityGrantInput<'a> {
    pub intent_type: &'a str,
    pub action_id: &'a str,
    pub protocol_version: &'a str,
    pub request_id: &'a str,
    pub actor: &'a str,
    pub bridge_mode: bool,
    pub project_id: i64,
    pub branch_name: &'a str,
    pub target_ref: Option<&'a str>,
    pub new_sha: Option<&'a str>,
    pub intent_payload: serde_json::Value,
}

impl CapabilityRepo {
    pub async fn open_default() -> Result<Self> {
        Ok(Self {
            db: crate::state::Db::open().await?,
        })
    }

    pub async fn latest_evidence_by_job_id(&self, job_id: i64) -> Result<Option<FailureCapsule>> {
        self.db.latest_evidence_by_job_id(job_id).await
    }

    pub async fn recent_release_attempts(&self, limit: i64) -> Result<Vec<ReleaseAttempt>> {
        self.db.recent_release_attempts(None, None, limit).await
    }

    pub async fn count_selector_misses_since(&self, since: &str) -> Result<i64> {
        self.db.count_selector_misses_since(since).await
    }

    pub async fn system_snapshot(&self) -> Result<CapabilitySystemSnapshot> {
        Ok(CapabilitySystemSnapshot {
            pools: self.db.list_pools().await?,
            recent_jobs: self.db.recent_job_events(10).await?,
            latest_release: self
                .db
                .recent_release_attempts(None, None, 1)
                .await?
                .into_iter()
                .next(),
        })
    }

    pub async fn ci_job_bottlenecks(
        &self,
        project_id: i64,
        ref_name: Option<&str>,
        limit: i64,
    ) -> Result<Vec<CiJobBottleneck>> {
        self.db
            .ci_job_bottlenecks(project_id, ref_name, limit)
            .await
    }

    pub async fn record_branch_capability_grant(
        &self,
        input: BranchCapabilityGrantInput<'_>,
    ) -> Result<String> {
        let grant_id = format!("grant-{}", uuid::Uuid::new_v4());
        let ref_name = qualify_branch_ref(input.branch_name);
        let payload = serde_json::json!({
            "protocol_version": input.protocol_version,
            "request_id": input.request_id,
            "actor": input.actor,
            "bridge_mode": input.bridge_mode,
            "scope": {
                "project_id": input.project_id,
                "ref_name": ref_name,
                "target_ref": input.target_ref,
                "new_sha": input.new_sha,
            },
            "intent_payload": input.intent_payload,
        });
        let payload = serde_json::to_string(&payload)?;
        let intent_id = self
            .db
            .record_capability_intent(crate::state::NewCapabilityIntent {
                request_id: input.request_id,
                intent_type: input.intent_type,
                action_id: input.action_id,
                project_id: Some(input.project_id),
                ref_name: Some(&ref_name),
                target_ref: input.target_ref,
                actor: input.actor,
                status: "executed",
                payload: &payload,
            })
            .await?;
        let expires_at = (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339();
        self.db
            .approve_capability_grant(crate::state::NewCapabilityGrant {
                intent_id,
                grant_id: &grant_id,
                action_id: input.action_id,
                project_id: Some(input.project_id),
                ref_name: &ref_name,
                new_sha: input.new_sha,
                required_grant: "agent_task",
                status: "approved",
                expires_at: &expires_at,
                payload: &payload,
            })
            .await?;
        Ok(grant_id)
    }
}

fn qualify_branch_ref(branch_name: &str) -> String {
    if branch_name.starts_with("refs/") {
        branch_name.to_string()
    } else {
        format!("refs/heads/{branch_name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_ref_qualification_preserves_full_refs() {
        assert_eq!(qualify_branch_ref("refs/heads/main"), "refs/heads/main");
        assert_eq!(
            qualify_branch_ref("feature/demo"),
            "refs/heads/feature/demo"
        );
    }
}
