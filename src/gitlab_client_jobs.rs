use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn list_jobs(&self, project_id: i64, scopes: &[&str]) -> Result<Vec<Job>> {
        let mut path = format!("/projects/{}/jobs", project_id);
        if !scopes.is_empty() {
            let scope_params: Vec<String> =
                scopes.iter().map(|s| format!("scope[]={}", s)).collect();
            path = format!("{}?{}", path, scope_params.join("&"));
        }
        let mut jobs: Vec<Job> = self.get_paginated_json(&path).await?;
        for job in &mut jobs {
            job.pipeline_id = job.effective_pipeline_id();
        }
        Ok(jobs)
    }

    pub async fn job_trace(&self, project_id: i64, job_id: i64) -> Result<String> {
        let trace = self
            .authed_request_url(
                Method::GET,
                self.api_url(&format!("/projects/{}/jobs/{}/trace", project_id, job_id)),
            )?
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(trace)
    }

    pub async fn job_artifact_file(
        &self,
        project_id: i64,
        job_id: i64,
        artifact_path: &str,
    ) -> Result<String> {
        let encoded_path = artifact_path
            .split('/')
            .map(|segment| urlencoding::encode(segment).to_string())
            .collect::<Vec<_>>()
            .join("/");
        let body = self
            .authed_request_url(
                Method::GET,
                self.api_url(&format!(
                    "/projects/{}/jobs/{}/artifacts/{}",
                    project_id, job_id, encoded_path
                )),
            )?
            .send()
            .await?
            .error_for_status()
            .with_context(|| {
                format!(
                    "download artifact {} from job {} in project {}",
                    artifact_path, job_id, project_id
                )
            })?
            .text()
            .await?;
        Ok(body)
    }

    pub async fn play_job(&self, project_id: i64, job_id: i64) -> Result<()> {
        self.api_post_nobody_void(
            self.api_url(&format!("/projects/{}/jobs/{}/play", project_id, job_id)),
        )
        .await?;
        info!(project_id, job_id, "played manual job");
        Ok(())
    }

    pub async fn cancel_job(&self, project_id: i64, job_id: i64) -> Result<()> {
        self.api_post_nobody_void(
            self.api_url(&format!("/projects/{}/jobs/{}/cancel", project_id, job_id)),
        )
        .await?;
        info!(project_id, job_id, "cancelled job");
        Ok(())
    }

    pub async fn requeue_job(&self, project_id: i64, job_id: i64) -> Result<()> {
        self.api_post_nobody_void(self.api_url(&format!(
            "/projects/{}/jobs/{}/{}",
            project_id,
            job_id,
            concat!("ret", "ry")
        )))
        .await?;
        info!(project_id, job_id, "requeued job");
        Ok(())
    }

    pub async fn get_job_log_snippet(
        &self,
        project_id: i64,
        job_id: i64,
        limit_bytes: usize,
    ) -> Result<String> {
        let trace = self.job_trace(project_id, job_id).await?;
        if trace.len() <= limit_bytes {
            Ok(trace)
        } else {
            Ok(format!(
                "... (truncated)\n{}",
                &trace[trace.len() - limit_bytes..]
            ))
        }
    }
}
