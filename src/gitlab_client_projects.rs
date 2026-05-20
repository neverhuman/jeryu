use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let projects: Vec<Project> = self.get_paginated_json("/projects?membership=true").await?;
        Ok(projects)
    }

    pub async fn get_project(&self, id: i64) -> Result<Project> {
        let project = self
            .api_get_json(self.api_url(&format!("/projects/{}", id)))
            .await?;
        Ok(project)
    }

    pub async fn create_project(&self, name: &str) -> Result<Project> {
        self.create_project_with_readme(name, true).await
    }

    pub async fn create_project_with_readme(
        &self,
        name: &str,
        initialize_with_readme: bool,
    ) -> Result<Project> {
        let project: Project = self
            .api_post_json(
                self.api_url("/projects"),
                &CreateProjectReq {
                    name,
                    visibility: "private",
                    initialize_with_readme,
                },
            )
            .await?;
        info!(project_id = project.id, "created project");
        Ok(project)
    }

    pub async fn create_project_bot(
        &self,
        project_id: i64,
        name: &str,
        scopes: &[&str],
        expires_at: &str,
        access_level: i32,
    ) -> Result<ProjectPatResp> {
        let resp: ProjectPatResp = self
            .api_post_json(
                self.api_url(&format!("/projects/{}/access_tokens", project_id)),
                &CreateProjectPatReq {
                    name,
                    scopes,
                    access_level,
                    expires_at,
                },
            )
            .await?;
        info!(
            project_id,
            bot_user_id = resp.user_id,
            "created ephemeral project bot user"
        );
        Ok(resp)
    }

    pub async fn create_file(
        &self,
        project_id: i64,
        branch: &str,
        file_path: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<()> {
        const ACTION: &str = "create";
        self.commit_file(
            project_id,
            branch,
            file_path,
            content,
            commit_message,
            ACTION,
        )
        .await
    }

    pub async fn update_file(
        &self,
        project_id: i64,
        branch: &str,
        file_path: &str,
        content: &str,
        commit_message: &str,
    ) -> Result<()> {
        self.commit_file(
            project_id,
            branch,
            file_path,
            content,
            commit_message,
            "update",
        )
        .await
    }

    pub async fn commit_file(
        &self,
        project_id: i64,
        branch: &str,
        file_path: &str,
        content: &str,
        commit_message: &str,
        action: &str,
    ) -> Result<()> {
        self.commit_actions(
            project_id,
            branch,
            commit_message,
            &[(action, file_path, content)],
        )
        .await
    }

    pub async fn update_files(
        &self,
        project_id: i64,
        branch: &str,
        commit_message: &str,
        files: &[(&str, &str)],
    ) -> Result<()> {
        let actions: Vec<(&str, &str, &str)> =
            files.iter().map(|(p, c)| ("update", *p, *c)).collect();
        self.commit_actions(project_id, branch, commit_message, &actions)
            .await
    }

    pub async fn commit_actions(
        &self,
        project_id: i64,
        branch: &str,
        commit_message: &str,
        files: &[(&str, &str, &str)],
    ) -> Result<()> {
        self.commit_actions_with_sha(project_id, branch, commit_message, files)
            .await
            .map(|_| ())
    }

    /// Commit a batch of file actions and return the GitLab commit SHA.
    pub async fn commit_actions_with_sha(
        &self,
        project_id: i64,
        branch: &str,
        commit_message: &str,
        files: &[(&str, &str, &str)],
    ) -> Result<String> {
        let actions: Vec<CommitAction> = files
            .iter()
            .map(|(action, path, content)| CommitAction {
                action,
                file_path: path,
                content,
            })
            .collect();

        let commit: CreateCommitResp = self
            .api_post_json(
                self.api_url(&format!("/projects/{}/repository/commits", project_id)),
                &CreateCommitReq {
                    branch,
                    commit_message,
                    actions,
                },
            )
            .await?;
        info!(
            project_id,
            branch,
            commit_sha = commit.id,
            files_count = files.len(),
            "committed files"
        );
        Ok(commit.id)
    }
}
