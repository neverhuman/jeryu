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

    pub(crate) async fn lint_ci_yaml(&self, project_id: i64, content: &str) -> Result<LintCiResp> {
        let response: serde_json::Value = self
            .api_post_json(
                self.api_url(&format!(
                    "/projects/{}/ci/lint?include_merged_yaml=true",
                    project_id
                )),
                &LintCiReq { content },
            )
            .await?;
        parse_lint_ci_response(response)
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

fn parse_lint_ci_response(response: serde_json::Value) -> Result<LintCiResp> {
    let object = response.as_object().ok_or_else(|| {
        anyhow::anyhow!("unexpected GitLab CI lint response shape: expected a JSON object")
    })?;

    let valid = object
        .get("valid")
        .or_else(|| object.get("status"))
        .map(parse_lint_valid_value)
        .transpose()?
        .ok_or_else(|| {
            anyhow::anyhow!("GitLab CI lint response did not include `valid` or `status`")
        })?;

    let errors = parse_lint_string_array(object.get("errors"), "errors")?;
    let warnings = parse_lint_string_array(object.get("warnings"), "warnings")?;
    let merged_yaml = match object.get("merged_yaml") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        Some(other) => {
            return Err(anyhow::anyhow!(
                "GitLab CI lint response field `merged_yaml` must be a string, got {}",
                value_kind(other)
            ));
        }
    };

    Ok(LintCiResp {
        valid,
        errors,
        warnings,
        merged_yaml,
    })
}

fn parse_lint_valid_value(value: &serde_json::Value) -> Result<bool> {
    match value {
        serde_json::Value::Bool(valid) => Ok(*valid),
        serde_json::Value::String(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "valid" => Ok(true),
            "false" | "invalid" => Ok(false),
            other => Err(anyhow::anyhow!(
                "GitLab CI lint response validity must be true/false or valid/invalid, got {other:?}"
            )),
        },
        other => Err(anyhow::anyhow!(
            "GitLab CI lint response validity must be a boolean or string, got {}",
            value_kind(other)
        )),
    }
}

fn parse_lint_string_array(
    value: Option<&serde_json::Value>,
    field_name: &str,
) -> Result<Vec<String>> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                serde_json::Value::String(value) => Ok(value.clone()),
                other => Err(anyhow::anyhow!(
                    "GitLab CI lint response field `{field_name}` must contain strings, got {}",
                    value_kind(other)
                )),
            })
            .collect(),
        Some(other) => Err(anyhow::anyhow!(
            "GitLab CI lint response field `{field_name}` must be an array, got {}",
            value_kind(other)
        )),
    }
}

fn value_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_ci_response_parses_valid_bool_response() {
        let response = serde_json::json!({
            "valid": true,
            "errors": [],
            "warnings": [],
            "merged_yaml": "---\nstages:\n  - test\n",
        });

        let parsed = parse_lint_ci_response(response).expect("parse lint response");

        assert!(parsed.valid);
        assert!(parsed.errors.is_empty());
        assert!(parsed.warnings.is_empty());
        assert_eq!(
            parsed.merged_yaml.as_deref(),
            Some("---\nstages:\n  - test\n")
        );
    }

    #[test]
    fn lint_ci_response_parses_invalid_status_response() {
        let response = serde_json::json!({
            "status": "invalid",
            "errors": ["jobs config should contain at least one visible job"],
            "warnings": ["warning: synthetic"],
            "merged_yaml": null,
        });

        let parsed = parse_lint_ci_response(response).expect("parse lint response");

        assert!(!parsed.valid);
        assert_eq!(
            parsed.errors,
            vec!["jobs config should contain at least one visible job".to_string()]
        );
        assert_eq!(parsed.warnings, vec!["warning: synthetic".to_string()]);
        assert!(parsed.merged_yaml.is_none());
    }
}
