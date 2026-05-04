//! Owner: GitLab REST Client subsystem
//! Proof: `cargo nextest run -p jeryu -- gitlab_client`
//! Invariants: HTTP calls preserve GitLab semantics, redact tokens, and surface status-specific failures.
//! GitLab REST API client for jeryu.
//!
//! Thin, purpose-built wrapper around reqwest. Every method maps to
//! one GitLab REST endpoint. No magic.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// Request / Response types (all at module level for derive macro compat)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CreateProjectPatReq<'a> {
    name: &'a str,
    scopes: &'a [&'a str],
    access_level: i32,
    expires_at: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct ProjectPatResp {
    pub id: i64,
    pub name: String,
    pub token: String,
    pub user_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RunnerCreated {
    pub id: i64,
    pub token: String,
}

#[derive(Serialize)]
struct CreateRunnerReq<'a> {
    description: &'a str,
    tag_list: &'a [&'a str],
    run_untagged: bool,
    runner_type: &'a str,
}

#[derive(Serialize)]
struct SetPausedReq {
    paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct RunnerManager {
    pub system_id: Option<String>,
    pub status: Option<String>,
    pub contacted_at: Option<String>,
}

#[derive(Deserialize)]
struct ResetTokenResp {
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub stage: String,
    #[serde(default)]
    pub allow_failure: bool,
    #[serde(skip)]
    pub pipeline_id: Option<i64>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_duration: Option<f64>,
    pub duration: Option<f64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub runner: Option<JobRunner>,
}

#[derive(Debug, Deserialize)]
pub struct JobRunner {
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Pipeline {
    pub id: i64,
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub status: String,
    pub web_url: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineBridge {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub downstream_pipeline: Option<PipelineRef>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineRef {
    pub id: i64,
    pub sha: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub status: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Serialize)]
struct CreateWebhookReq<'a> {
    url: &'a str,
    token: &'a str,
    job_events: bool,
    pipeline_events: bool,
    push_events: bool,
    merge_requests_events: bool,
}

#[derive(Deserialize)]
struct WebhookResp {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub web_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Issue {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub labels: Vec<String>,
    pub web_url: String,
}

#[derive(Serialize)]
struct CreateIssueReq<'a> {
    title: &'a str,
    description: &'a str,
    labels: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_ids: Option<Vec<i64>>,
}

#[derive(Serialize)]
struct UpdateLabelsReq {
    labels: String,
}

#[derive(Serialize)]
struct NoteReq<'a> {
    body: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub web_url: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Serialize)]
struct CreateMrReq<'a> {
    source_branch: &'a str,
    target_branch: &'a str,
    title: &'a str,
    description: &'a str,
    remove_source_branch: bool,
}

#[derive(Serialize)]
struct CreateBranchReq<'a> {
    branch: &'a str,
    #[serde(rename = "ref")]
    ref_name: &'a str,
}

#[derive(Serialize)]
struct CreateProjectReq<'a> {
    name: &'a str,
    visibility: &'a str,
    initialize_with_readme: bool,
}

#[derive(Serialize)]
struct CommitAction<'a> {
    action: &'a str,
    file_path: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct CreateCommitReq<'a> {
    branch: &'a str,
    commit_message: &'a str,
    actions: Vec<CommitAction<'a>>,
}

#[derive(Deserialize)]
struct CreateCommitResp {
    id: String,
}

#[derive(Serialize)]
struct CreatePipelineReq<'a> {
    #[serde(rename = "ref")]
    pub ref_name: &'a str,
    pub variables: Vec<PipelineVariable<'a>>,
}

#[derive(Serialize)]
struct PipelineVariable<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Deserialize)]
struct PipelineResp {
    pub id: i64,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitlabClient {
    base_url: String,
    client: Client,
    pat: Option<String>,
}

impl GitlabClient {
    pub fn new(base_url: &str, pat: Option<String>) -> Self {
        let insecure_tls = insecure_tls_enabled_from_env();
        Self::new_with_tls_policy(base_url, pat, insecure_tls)
    }

    pub fn new_with_tls_policy(base_url: &str, pat: Option<String>, insecure_tls: bool) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .danger_accept_invalid_certs(insecure_tls)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            pat,
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/v4{}", self.base_url, path)
    }

    async fn get_paginated_json<T>(&self, path: &str, pat: &str) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut page = 1_u32;
        let per_page = 100_u32;
        let mut items = Vec::new();

        loop {
            let url = self.paginated_url(path, page, per_page);
            let resp = self
                .client
                .get(&url)
                .header("PRIVATE-TOKEN", pat)
                .send()
                .await?
                .error_for_status()?;
            let next_page = resp
                .headers()
                .get("x-next-page")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        trimmed.parse::<u32>().ok()
                    }
                });
            let batch: Vec<T> = resp.json().await?;
            let batch_len = batch.len();
            items.extend(batch);
            match next_page {
                Some(next_page) if next_page > page => {
                    page = next_page;
                }
                _ if batch_len == per_page as usize => {
                    page += 1;
                }
                _ => break,
            }
        }

        Ok(items)
    }

    fn paginated_url(&self, path: &str, page: u32, per_page: u32) -> String {
        let url = self.api_url(path);
        if url.contains('?') {
            format!("{url}&per_page={per_page}&page={page}")
        } else {
            format!("{url}?per_page={per_page}&page={page}")
        }
    }

    fn pat_value(&self) -> Result<String> {
        self.pat
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no PAT configured — run `jeryu bootstrap` first"))
    }

    pub fn pat_value_for_clone(&self) -> Option<String> {
        self.pat.clone()
    }

    // -- Health -------------------------------------------------------------

    pub async fn is_ready(&self) -> bool {
        for path in ["/help", "/users/sign_in"] {
            let url = format!("{}{}", self.base_url, path);
            if let Ok(resp) = self.client.get(&url).send().await {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    return true;
                }
            }
        }

        false
    }

    // -- Runners ------------------------------------------------------------

    pub async fn create_runner(
        &self,
        description: &str,
        tag_list: &[&str],
        run_untagged: bool,
        runner_type: &str,
    ) -> Result<RunnerCreated> {
        let pat = self.pat_value()?;
        let resp: RunnerCreated = self
            .client
            .post(self.api_url("/user/runners"))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateRunnerReq {
                description,
                tag_list,
                run_untagged,
                runner_type,
            })
            .send()
            .await
            .context("create runner request")?
            .error_for_status()
            .context("create runner response")?
            .json()
            .await?;
        info!(id = resp.id, "created runner");
        Ok(resp)
    }

    pub async fn set_runner_paused(&self, runner_id: i64, paused: bool) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .put(self.api_url(&format!("/runners/{}", runner_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&SetPausedReq { paused })
            .send()
            .await
            .context("set runner paused")?
            .error_for_status()?;
        info!(runner_id, paused, "updated runner paused state");
        Ok(())
    }

    pub async fn list_runner_managers(&self, runner_id: i64) -> Result<Vec<RunnerManager>> {
        let pat = self.pat_value()?;
        let managers: Vec<RunnerManager> = self
            .client
            .get(self.api_url(&format!("/runners/{}/managers", runner_id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(managers)
    }

    pub async fn delete_runner(&self, runner_id: i64) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .delete(self.api_url(&format!("/runners/{}", runner_id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(runner_id, "deleted runner");
        Ok(())
    }

    pub async fn reset_runner_token(&self, runner_id: i64) -> Result<String> {
        let pat = self.pat_value()?;
        let resp: ResetTokenResp = self
            .client
            .post(self.api_url(&format!(
                "/runners/{}/reset_authentication_token",
                runner_id
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!(runner_id, "reset runner auth token");
        Ok(resp.token)
    }

    // -- Jobs ---------------------------------------------------------------

    pub async fn list_jobs(&self, project_id: i64, scopes: &[&str]) -> Result<Vec<Job>> {
        let pat = self.pat_value()?;
        let mut path = format!("/projects/{}/jobs", project_id);
        if !scopes.is_empty() {
            let scope_params: Vec<String> =
                scopes.iter().map(|s| format!("scope[]={}", s)).collect();
            path = format!("{}?{}", path, scope_params.join("&"));
        }
        let jobs: Vec<Job> = self.get_paginated_json(&path, &pat).await?;
        Ok(jobs)
    }

    pub async fn job_trace(&self, project_id: i64, job_id: i64) -> Result<String> {
        let pat = self.pat_value()?;
        let trace = self
            .client
            .get(self.api_url(&format!("/projects/{}/jobs/{}/trace", project_id, job_id)))
            .header("PRIVATE-TOKEN", &pat)
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
        let pat = self.pat_value()?;
        let encoded_path = artifact_path
            .split('/')
            .map(|segment| urlencoding::encode(segment).to_string())
            .collect::<Vec<_>>()
            .join("/");
        let body = self
            .client
            .get(self.api_url(&format!(
                "/projects/{}/jobs/{}/artifacts/{}",
                project_id, job_id, encoded_path
            )))
            .header("PRIVATE-TOKEN", &pat)
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
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!("/projects/{}/jobs/{}/play", project_id, job_id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, job_id, "played manual job");
        Ok(())
    }

    pub async fn cancel_job(&self, project_id: i64, job_id: i64) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!("/projects/{}/jobs/{}/cancel", project_id, job_id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, job_id, "cancelled job");
        Ok(())
    }

    pub async fn retry_job(&self, project_id: i64, job_id: i64) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!("/projects/{}/jobs/{}/retry", project_id, job_id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, job_id, "retried job");
        Ok(())
    }

    // -- Webhooks -----------------------------------------------------------

    pub async fn create_group_webhook(
        &self,
        group_id: i64,
        url: &str,
        secret_token: &str,
    ) -> Result<i64> {
        let pat = self.pat_value()?;
        let resp: WebhookResp = self
            .client
            .post(self.api_url(&format!("/groups/{}/hooks", group_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateWebhookReq {
                url,
                token: secret_token,
                job_events: true,
                pipeline_events: true,
                push_events: true,
                merge_requests_events: true,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!(webhook_id = resp.id, "created group webhook");
        Ok(resp.id)
    }

    // -- Projects -----------------------------------------------------------

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let pat = self.pat_value()?;
        let projects: Vec<Project> = self
            .get_paginated_json("/projects?membership=true", &pat)
            .await?;
        Ok(projects)
    }

    pub async fn get_project(&self, id: i64) -> Result<Project> {
        let pat = self.pat_value()?;
        let project: Project = self
            .client
            .get(self.api_url(&format!("/projects/{}", id)))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(project)
    }

    pub async fn create_project(&self, name: &str) -> Result<Project> {
        let pat = self.pat_value()?;
        let project: Project = self
            .client
            .post(self.api_url("/projects"))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateProjectReq {
                name,
                visibility: "private",
                initialize_with_readme: true,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
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
        let pat = self.pat_value()?;
        let resp: ProjectPatResp = self
            .client
            .post(self.api_url(&format!("/projects/{}/access_tokens", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateProjectPatReq {
                name,
                scopes,
                access_level,
                expires_at,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
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
        self.commit_actions(
            project_id,
            branch,
            commit_message,
            &[("create", file_path, content)],
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
        self.commit_actions(
            project_id,
            branch,
            commit_message,
            &[("update", file_path, content)],
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
        let pat = self.pat_value()?;

        let actions: Vec<CommitAction> = files
            .iter()
            .map(|(action, path, content)| CommitAction {
                action,
                file_path: path,
                content,
            })
            .collect();

        let commit: CreateCommitResp = self
            .client
            .post(self.api_url(&format!("/projects/{}/repository/commits", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateCommitReq {
                branch,
                commit_message,
                actions,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
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

    // -- Issues -------------------------------------------------------------

    pub async fn create_issue(
        &self,
        project_id: i64,
        title: &str,
        description: &str,
        labels: &[&str],
        assignee_id: Option<i64>,
    ) -> Result<Issue> {
        let pat = self.pat_value()?;
        let assignee_ids = assignee_id.map(|id| vec![id]);

        let issue: Issue = self
            .client
            .post(self.api_url(&format!("/projects/{}/issues", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateIssueReq {
                title,
                description,
                labels: labels.join(","),
                assignee_ids,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!(project_id, issue_iid = issue.iid, "created issue");
        Ok(issue)
    }

    pub async fn list_issues_by_labels(
        &self,
        project_id: i64,
        labels: &[&str],
        state: Option<&str>,
    ) -> Result<Vec<Issue>> {
        let pat = self.pat_value()?;
        let mut params = vec!["per_page=100".to_string()];
        if !labels.is_empty() {
            params.push(format!("labels={}", urlencoding::encode(&labels.join(","))));
        }
        if let Some(state) = state {
            params.push(format!("state={}", urlencoding::encode(state)));
        }
        let issues: Vec<Issue> = self
            .client
            .get(self.api_url(&format!(
                "/projects/{}/issues?{}",
                project_id,
                params.join("&")
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(issues)
    }

    pub async fn update_issue_labels(
        &self,
        project_id: i64,
        issue_iid: i64,
        labels: &[&str],
    ) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .put(self.api_url(&format!("/projects/{}/issues/{}", project_id, issue_iid)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&UpdateLabelsReq {
                labels: labels.join(","),
            })
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn comment_on_issue(
        &self,
        project_id: i64,
        issue_iid: i64,
        body: &str,
    ) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!(
                "/projects/{}/issues/{}/notes",
                project_id, issue_iid
            )))
            .header("PRIVATE-TOKEN", &pat)
            .json(&NoteReq { body })
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // -- Merge Requests -----------------------------------------------------

    pub async fn create_merge_request(
        &self,
        project_id: i64,
        source_branch: &str,
        target_branch: &str,
        title: &str,
        description: &str,
    ) -> Result<MergeRequest> {
        let pat = self.pat_value()?;
        let mr: MergeRequest = self
            .client
            .post(self.api_url(&format!("/projects/{}/merge_requests", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateMrReq {
                source_branch,
                target_branch,
                title,
                description,
                remove_source_branch: true,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        info!(project_id, mr_iid = mr.iid, "created merge request");
        Ok(mr)
    }

    pub async fn accept_merge_request(&self, project_id: i64, mr_iid: i64) -> Result<()> {
        let pat = self.pat_value()?;
        let url = self.api_url(&format!(
            "/projects/{}/merge_requests/{}/merge",
            project_id, mr_iid
        ));
        let resp = self
            .client
            .put(&url)
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?;

        if resp.status().as_u16() == 405 {
            self.client
                .post(&url)
                .header("PRIVATE-TOKEN", &pat)
                .send()
                .await?
                .error_for_status()?;
        } else {
            resp.error_for_status()?;
        }
        info!(project_id, mr_iid, "accepted merge request");
        Ok(())
    }

    // -- Branches -----------------------------------------------------------

    pub async fn create_branch(
        &self,
        project_id: i64,
        branch_name: &str,
        ref_name: &str,
    ) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!("/projects/{}/repository/branches", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreateBranchReq {
                branch: branch_name,
                ref_name,
            })
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, branch_name, "created branch");
        Ok(())
    }

    pub async fn delete_branch(&self, project_id: i64, branch_name: &str) -> Result<()> {
        let pat = self.pat_value()?;
        // Branch names need URL encoding
        let encoded_branch = urlencoding::encode(branch_name);
        self.client
            .delete(self.api_url(&format!(
                "/projects/{}/repository/branches/{}",
                project_id, encoded_branch
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, branch_name, "deleted branch");
        Ok(())
    }

    // -- Pipelines ----------------------------------------------------------

    pub async fn trigger_pipeline(
        &self,
        project_id: i64,
        ref_name: &str,
        variables: Vec<(&str, &str)>,
    ) -> Result<i64> {
        let pat = self.pat_value()?;
        let vars = variables
            .into_iter()
            .map(|(k, v)| PipelineVariable { key: k, value: v })
            .collect();

        let resp: PipelineResp = self
            .client
            .post(self.api_url(&format!("/projects/{}/pipeline", project_id)))
            .header("PRIVATE-TOKEN", &pat)
            .json(&CreatePipelineReq {
                ref_name,
                variables: vars,
            })
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        info!(project_id, pipeline_id = resp.id, "triggered pipeline");
        Ok(resp.id)
    }

    pub async fn list_pipelines(
        &self,
        project_id: i64,
        ref_name: Option<&str>,
    ) -> Result<Vec<Pipeline>> {
        let pat = self.pat_value()?;
        let mut path = format!("/projects/{}/pipelines", project_id);
        if let Some(ref_name) = ref_name {
            path.push_str(&format!("?ref={}", urlencoding::encode(ref_name)));
        }
        let pipelines: Vec<Pipeline> = self.get_paginated_json(&path, &pat).await?;
        Ok(pipelines)
    }

    pub async fn list_pipeline_jobs(&self, project_id: i64, pipeline_id: i64) -> Result<Vec<Job>> {
        let pat = self.pat_value()?;
        let mut jobs: Vec<Job> = self
            .get_paginated_json(
                &format!("/projects/{}/pipelines/{}/jobs", project_id, pipeline_id),
                &pat,
            )
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
        let pat = self.pat_value()?;
        let bridges: Vec<PipelineBridge> = self
            .get_paginated_json(
                &format!("/projects/{}/pipelines/{}/bridges", project_id, pipeline_id),
                &pat,
            )
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
        let pat = self.pat_value()?;
        let pipeline: Pipeline = self
            .client
            .get(self.api_url(&format!(
                "/projects/{}/pipelines/{}",
                project_id, pipeline_id
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(pipeline)
    }

    pub async fn cancel_pipeline(&self, project_id: i64, pipeline_id: i64) -> Result<()> {
        let pat = self.pat_value()?;
        self.client
            .post(self.api_url(&format!(
                "/projects/{}/pipelines/{}/cancel",
                project_id, pipeline_id
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?;
        info!(project_id, pipeline_id, "cancelled pipeline");
        Ok(())
    }

    pub async fn get_merge_request(&self, project_id: i64, mr_iid: i64) -> Result<MergeRequest> {
        let pat = self.pat_value()?;
        let mr: MergeRequest = self
            .client
            .get(self.api_url(&format!(
                "/projects/{}/merge_requests/{}",
                project_id, mr_iid
            )))
            .header("PRIVATE-TOKEN", &pat)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(mr)
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

fn insecure_tls_enabled_from_env() -> bool {
    std::env::var("JERYU_GITLAB_INSECURE_TLS")
        .ok()
        .is_some_and(|value| insecure_tls_enabled_from_value(&value))
}

fn insecure_tls_enabled_from_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insecure_tls_is_opt_in_only() {
        assert!(!insecure_tls_enabled_from_value(""));
        assert!(!insecure_tls_enabled_from_value("false"));
        assert!(!insecure_tls_enabled_from_value("0"));
        assert!(insecure_tls_enabled_from_value("1"));
        assert!(insecure_tls_enabled_from_value("true"));
    }

    #[test]
    fn client_constructor_keeps_explicit_tls_policy() {
        let secure = GitlabClient::new_with_tls_policy("http://localhost:8929/", None, false);
        assert_eq!(secure.base_url, "http://localhost:8929");
        let insecure = GitlabClient::new_with_tls_policy("http://localhost:8929/", None, true);
        assert_eq!(insecure.base_url, "http://localhost:8929");
    }
}
