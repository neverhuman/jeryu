use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn create_branch(
        &self,
        project_id: i64,
        branch_name: &str,
        ref_name: &str,
    ) -> Result<()> {
        self.api_post_void(
            self.api_url(&format!("/projects/{}/repository/branches", project_id)),
            &CreateBranchReq {
                branch: branch_name,
                ref_name,
            },
        )
        .await?;
        info!(project_id, branch_name, "created branch");
        Ok(())
    }

    pub async fn delete_branch(&self, project_id: i64, branch_name: &str) -> Result<()> {
        let encoded_branch = urlencoding::encode(branch_name);
        self.api_delete_void(self.api_url(&format!(
            "/projects/{}/repository/branches/{}",
            project_id, encoded_branch
        )))
        .await?;
        info!(project_id, branch_name, "deleted branch");
        Ok(())
    }

    pub async fn protect_branch_mr_only(&self, project_id: i64, branch_name: &str) -> Result<()> {
        let result = self
            .api_post_void(
                self.api_url(&format!("/projects/{}/protected_branches", project_id)),
                &ProtectBranchReq {
                    name: branch_name,
                    push_access_level: 0,
                    merge_access_level: 40,
                    allow_force_push: false,
                },
            )
            .await;
        match result {
            Ok(()) => {
                info!(project_id, branch_name, "protected branch");
                Ok(())
            }
            Err(err) if err.to_string().contains("409") => Ok(()),
            Err(err) => Err(err),
        }
    }
}
