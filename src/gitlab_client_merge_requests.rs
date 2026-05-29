use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn list_open_merge_requests_for_branches(
        &self,
        project_id: i64,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<Vec<MergeRequest>> {
        let path = format!(
            "/projects/{}/merge_requests?state=opened&source_branch={}&target_branch={}",
            project_id,
            urlencoding::encode(source_branch),
            urlencoding::encode(target_branch)
        );
        self.get_paginated_json(&path).await
    }

    pub async fn create_merge_request(
        &self,
        project_id: i64,
        source_branch: &str,
        target_branch: &str,
        title: &str,
        description: &str,
    ) -> Result<MergeRequest> {
        let mr: MergeRequest = self
            .api_post_json(
                self.api_url(&format!("/projects/{}/merge_requests", project_id)),
                &CreateMrReq {
                    source_branch,
                    target_branch,
                    title,
                    description,
                    remove_source_branch: true,
                },
            )
            .await?;
        info!(project_id, mr_iid = mr.iid, "created merge request");
        Ok(mr)
    }

    pub async fn update_merge_request_metadata(
        &self,
        project_id: i64,
        mr_iid: i64,
        title: &str,
        description: &str,
    ) -> Result<MergeRequest> {
        #[derive(serde::Serialize)]
        struct UpdateMrReq<'a> {
            title: &'a str,
            description: &'a str,
        }

        let mr: MergeRequest = self
            .authed_request_url(
                Method::PUT,
                self.api_url(&format!(
                    "/projects/{}/merge_requests/{}",
                    project_id, mr_iid
                )),
            )?
            .json(&UpdateMrReq { title, description })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!(
            project_id,
            mr_iid = mr.iid,
            "updated merge request metadata"
        );
        Ok(mr)
    }

    pub async fn accept_merge_request(&self, project_id: i64, mr_iid: i64) -> Result<()> {
        let url = self.api_url(&format!(
            "/projects/{}/merge_requests/{}/merge",
            project_id, mr_iid
        ));
        let resp = self
            .authed_request_url(Method::PUT, url.clone())?
            .send()
            .await?;

        if resp.status().as_u16() == 405 {
            self.authed_request_url(Method::POST, url)?
                .send()
                .await?
                .error_for_status()?;
        } else {
            resp.error_for_status()?;
        }
        info!(project_id, mr_iid, "accepted merge request");
        Ok(())
    }

    pub async fn get_merge_request(&self, project_id: i64, mr_iid: i64) -> Result<MergeRequest> {
        let mr = self
            .api_get_json(self.api_url(&format!(
                "/projects/{}/merge_requests/{}",
                project_id, mr_iid
            )))
            .await?;
        Ok(mr)
    }
}
