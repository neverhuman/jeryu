use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn trigger_pipeline_details(
        &self,
        project_id: i64,
        ref_name: &str,
        variables: Vec<(&str, &str)>,
    ) -> Result<Pipeline> {
        let vars: Vec<PipelineVariable> = variables
            .into_iter()
            .map(|(k, v)| PipelineVariable { key: k, value: v })
            .collect();
        let resp: PipelineResp = self
            .api_post_json(
                self.api_url(&format!("/projects/{}/pipeline", project_id)),
                &CreatePipelineReq {
                    ref_name,
                    variables: vars,
                },
            )
            .await?;
        let pipeline = Pipeline {
            id: resp.id,
            sha: resp.sha,
            ref_name: resp.ref_name,
            status: resp.status,
            web_url: resp.web_url,
            source: resp.source,
        };
        info!(project_id, pipeline_id = pipeline.id, "triggered pipeline");
        Ok(pipeline)
    }

    pub async fn trigger_pipeline(
        &self,
        project_id: i64,
        ref_name: &str,
        variables: Vec<(&str, &str)>,
    ) -> Result<i64> {
        Ok(self
            .trigger_pipeline_details(project_id, ref_name, variables)
            .await?
            .id)
    }

    pub async fn list_pipelines(
        &self,
        project_id: i64,
        ref_name: Option<&str>,
    ) -> Result<Vec<Pipeline>> {
        let mut path = format!("/projects/{}/pipelines", project_id);
        if let Some(ref_name) = ref_name {
            path.push_str(&format!("?ref={}", urlencoding::encode(ref_name)));
        }
        let pipelines: Vec<Pipeline> = self.get_paginated_json(&path).await?;
        Ok(pipelines)
    }

    pub async fn list_pipeline_variables(
        &self,
        project_id: i64,
        pipeline_id: i64,
    ) -> Result<Vec<PipelineVariableValue>> {
        let variables: Vec<PipelineVariableValue> = self
            .get_paginated_json(&format!(
                "/projects/{}/pipelines/{}/variables",
                project_id, pipeline_id
            ))
            .await?;
        Ok(variables)
    }

    pub async fn list_pipeline_jobs(&self, project_id: i64, pipeline_id: i64) -> Result<Vec<Job>> {
        let mut jobs: Vec<Job> = self
            .get_paginated_json(&format!(
                "/projects/{}/pipelines/{}/jobs",
                project_id, pipeline_id
            ))
            .await?;
        for job in &mut jobs {
            job.pipeline_id = Some(pipeline_id);
        }
        Ok(jobs)
    }

    pub async fn list_pipeline_bridges(
        &self,
        project_id: i64,
        pipeline_id: i64,
    ) -> Result<Vec<PipelineBridge>> {
        let bridges: Vec<PipelineBridge> = self
            .get_paginated_json(&format!(
                "/projects/{}/pipelines/{}/bridges",
                project_id, pipeline_id
            ))
            .await?;
        Ok(bridges)
    }

    pub async fn list_pipeline_jobs_with_downstream(
        &self,
        project_id: i64,
        pipeline_id: i64,
    ) -> Result<Vec<Job>> {
        let mut all_jobs = Vec::new();
        let mut stack = vec![pipeline_id];
        let mut seen = std::collections::BTreeSet::new();

        while let Some(current_pipeline_id) = stack.pop() {
            if !seen.insert(current_pipeline_id) {
                continue;
            }
            all_jobs.extend(
                self.list_pipeline_jobs(project_id, current_pipeline_id)
                    .await?,
            );
            for bridge in self
                .list_pipeline_bridges(project_id, current_pipeline_id)
                .await?
            {
                if let Some(downstream) = bridge.downstream_pipeline {
                    stack.push(downstream.id);
                }
            }
        }

        Ok(all_jobs)
    }

    pub async fn get_pipeline(&self, project_id: i64, pipeline_id: i64) -> Result<Pipeline> {
        let pipeline = self
            .api_get_json(self.api_url(&format!(
                "/projects/{}/pipelines/{}",
                project_id, pipeline_id
            )))
            .await?;
        Ok(pipeline)
    }

    pub async fn cancel_pipeline(&self, project_id: i64, pipeline_id: i64) -> Result<()> {
        self.api_post_nobody_void(self.api_url(&format!(
            "/projects/{}/pipelines/{}/cancel",
            project_id, pipeline_id
        )))
        .await?;
        info!(project_id, pipeline_id, "cancelled pipeline");
        Ok(())
    }
}
