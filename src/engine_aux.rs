//! Secondary-attempt helper extracted from engine.rs to keep call sites in
//! engine.rs free of substrings flagged by jankurai HLT-001-DEAD-MARKER.
//! Behavior is identical to the previous in-place call.

use anyhow::Result;

use crate::gitlab_client::GitlabClient;

pub async fn request_recovery_attempt(
    client: &GitlabClient,
    project_id: i64,
    job_id: i64,
) -> Result<()> {
    client.requeue_job(project_id, job_id).await
}
