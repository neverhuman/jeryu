use crate::release;
use crate::state::{JobEvent, Pool, TrackedPipeline};

#[allow(clippy::too_many_arguments)] // demo fixture: flat positional schema by design
pub(crate) fn demo_pool(
    name: &str,
    gitlab_runner_id: i64,
    auth_token: &str,
    tags: &str,
    min_warm: i64,
    max_managers: i64,
    concurrent: i64,
    request_concurrency: i64,
    trust_tier: &str,
) -> Pool {
    Pool {
        name: name.into(),
        gitlab_runner_id,
        auth_token: auth_token.into(),
        tags: tags.into(),
        executor: "docker".into(),
        min_warm,
        max_managers,
        concurrent,
        request_concurrency,
        paused: false,
        trust_tier: trust_tier.into(),
    }
}

pub(crate) fn demo_pipeline(pipeline_id: i64, status: &str, updated_at: String) -> TrackedPipeline {
    TrackedPipeline {
        pipeline_id,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        ref_name: "main".into(),
        sha: "9c3f2d4e0b9f5d1d7cc8".into(),
        status: status.into(),
        updated_at,
    }
}

pub(crate) fn demo_job_event(
    job_id: i64,
    status: &str,
    job_name: &str,
    pool_name: &str,
    system_id: &str,
    queued_duration: Option<f64>,
    received_at: String,
) -> JobEvent {
    JobEvent {
        job_id,
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        pipeline_id: Some(8_013),
        status: status.into(),
        job_name: Some(job_name.into()),
        pool_name: Some(pool_name.into()),
        system_id: Some(system_id.into()),
        queued_duration,
        received_at,
    }
}

pub(crate) fn demo_evidence_record(
    id: i64,
    event_type: &str,
    job_id: i64,
    stage: &str,
    classification: &str,
    payload: &str,
    created_at: String,
) -> crate::state::EvidenceRecord {
    crate::state::EvidenceRecord {
        id,
        event_type: event_type.into(),
        project_id: release::DEFAULT_RELEASE_PROJECT_ID,
        job_id,
        pipeline_id: Some(8_013),
        commit_sha: "9c3f2d4e0b9f5d1d7cc8".into(),
        ref_name: "main".into(),
        stage: stage.into(),
        exit_code: 0,
        failure_kind: "none".into(),
        classification: classification.into(),
        created_at,
        payload: payload.into(),
    }
}

pub(crate) fn demo_secret_audit_event(
    id: i64,
    target: &str,
    action: &str,
    detail: &str,
    created_at: String,
) -> crate::state::SecretAuditEvent {
    crate::state::SecretAuditEvent {
        id: Some(id),
        repo_name: "jeryu".into(),
        version: "v3.0.1".into(),
        target: target.into(),
        action: action.into(),
        status: "ok".into(),
        detail: detail.into(),
        created_at,
    }
}
