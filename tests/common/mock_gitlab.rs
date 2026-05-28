//! Owner: Test helpers — offline mock GitLab HTTP server
//! Proof: used by `tests/mock_lifecycle_tests.rs`
//! Invariants:
//!   - All endpoints return JSON compatible with `GitlabClient`'s Deserialize impls.
//!   - State mutations are synchronous (single Mutex); safe for multi-turn polling.
//!   - Server binds port 0 (OS-assigned) so tests never conflict.
//!   - Drop of `MockGitlabServer` shuts down the background Tokio task.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// In-memory state
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MockRunner {
    pub id: i64,
    pub token: String,
    pub paused: bool,
    pub tags: Vec<String>,
    pub run_untagged: bool,
}

#[derive(Clone, Debug)]
pub struct MockProject {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct MockPipeline {
    pub id: i64,
    pub project_id: i64,
    pub ref_name: String,
    pub status: String,
    pub sha: String,
    pub yaml_errors: Option<String>,
}

#[derive(Clone, Debug)]
struct MockBranchState {
    sha: String,
    ci_job_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct MockJob {
    pub id: i64,
    pub pipeline_id: i64,
    pub name: String,
    pub stage: String,
    /// One of: "created", "pending", "running", "success", "failed", "canceled"
    pub status: String,
    pub trace: String,
}

#[derive(Clone, Debug, Default)]
pub struct MockLintResponse {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub merged_yaml: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MockCreateBranchReq {
    branch: String,
    #[serde(rename = "ref")]
    ref_name: String,
}

#[derive(Debug, Deserialize)]
struct MockCommitActionReq {
    file_path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MockCommitReq {
    branch: Option<String>,
    actions: Vec<MockCommitActionReq>,
}

#[derive(Debug, Deserialize)]
struct MockLintReq {
    content: String,
}

#[derive(Default, Debug)]
pub struct MockGitlabInner {
    next_id: i64,
    pub runners: HashMap<i64, MockRunner>,
    pub projects: HashMap<i64, MockProject>,
    branches: HashMap<(i64, String), MockBranchState>,
    pub pipelines: HashMap<i64, MockPipeline>,
    pub jobs: HashMap<i64, MockJob>,
    pub lint_response: Option<MockLintResponse>,
    pub commit_job_name_override: Option<String>,
    pub commit_pipeline_status_override: Option<String>,
    pub commit_yaml_errors_override: Option<String>,
}

impl MockGitlabInner {
    pub fn next_id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    /// Advance all jobs that are not yet in a terminal state to `status`.
    pub fn advance_jobs(&mut self, status: &str) {
        for job in self.jobs.values_mut() {
            if job.status != "success" && job.status != "failed" && job.status != "canceled" {
                job.status = status.to_string();
            }
        }
        // Mirror on parent pipelines.
        for pipeline in self.pipelines.values_mut() {
            if pipeline.status != "success" && pipeline.status != "failed" {
                pipeline.status = status.to_string();
            }
        }
    }

    /// Set a specific job's status (used for the failure→retry cycle test).
    pub fn set_job_status(&mut self, job_id: i64, status: &str) {
        if let Some(job) = self.jobs.get_mut(&job_id) {
            job.status = status.to_string();
            // Mirror on the parent pipeline.
            let pid = job.pipeline_id;
            if let Some(p) = self.pipelines.get_mut(&pid) {
                p.status = status.to_string();
            }
        }
    }

    /// Add a pipeline + one job for `project_id`. Returns (pipeline_id, job_id).
    pub fn add_pipeline(
        &mut self,
        project_id: i64,
        ref_name: &str,
        job_name: &str,
        initial_status: &str,
        trace: &str,
    ) -> (i64, i64) {
        let sha = format!("deadbeef{:08x}", self.next_id + 1);
        self.add_pipeline_with_sha(
            project_id,
            ref_name,
            job_name,
            initial_status,
            trace,
            &sha,
            None,
        )
    }

    fn branch_key(project_id: i64, branch_name: &str) -> (i64, String) {
        (project_id, branch_name.to_string())
    }

    fn branch_state_mut(
        &mut self,
        project_id: i64,
        branch_name: &str,
    ) -> Option<&mut MockBranchState> {
        self.branches
            .get_mut(&Self::branch_key(project_id, branch_name))
    }

    fn set_branch_state(
        &mut self,
        project_id: i64,
        branch_name: &str,
        sha: String,
        ci_job_name: Option<String>,
    ) {
        self.branches.insert(
            Self::branch_key(project_id, branch_name),
            MockBranchState { sha, ci_job_name },
        );
    }

    fn add_pipeline_with_sha(
        &mut self,
        project_id: i64,
        ref_name: &str,
        job_name: &str,
        initial_status: &str,
        trace: &str,
        sha: &str,
        yaml_errors: Option<String>,
    ) -> (i64, i64) {
        let pid = self.next_id();
        let jid = self.next_id();
        self.pipelines.insert(
            pid,
            MockPipeline {
                id: pid,
                project_id,
                ref_name: ref_name.to_string(),
                status: initial_status.to_string(),
                sha: sha.to_string(),
                yaml_errors,
            },
        );
        self.jobs.insert(
            jid,
            MockJob {
                id: jid,
                pipeline_id: pid,
                name: job_name.to_string(),
                stage: "test".to_string(),
                status: initial_status.to_string(),
                trace: trace.to_string(),
            },
        );
        (pid, jid)
    }
}

// ---------------------------------------------------------------------------
// Shared state alias used by axum handlers
// ---------------------------------------------------------------------------

pub type GitlabState = Arc<Mutex<MockGitlabInner>>;

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

async fn health_check() -> StatusCode {
    StatusCode::OK
}

// POST /api/v4/user/runners
async fn create_runner(
    State(state): State<GitlabState>,
    body: Option<axum::extract::Json<Value>>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    let id = s.next_id();
    let token = format!("mock-runner-token-{id}");
    let tags = body
        .as_ref()
        .and_then(|b| b.get("tag_list"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let run_untagged = body
        .as_ref()
        .and_then(|b| b.get("run_untagged"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    s.runners.insert(
        id,
        MockRunner {
            id,
            token: token.clone(),
            paused: false,
            tags,
            run_untagged,
        },
    );
    (
        StatusCode::CREATED,
        Json(json!({ "id": id, "token": token })),
    )
}

// PUT /api/v4/runners/:id
async fn update_runner(
    State(state): State<GitlabState>,
    Path(id): Path<i64>,
    body: Option<axum::extract::Json<Value>>,
) -> StatusCode {
    let mut s = state.lock().unwrap();
    if let Some(runner) = s.runners.get_mut(&id) {
        if let Some(paused) = body
            .as_ref()
            .and_then(|b| b.get("paused"))
            .and_then(|v| v.as_bool())
        {
            runner.paused = paused;
        }
        if let Some(run_untagged) = body
            .as_ref()
            .and_then(|b| b.get("run_untagged"))
            .and_then(|v| v.as_bool())
        {
            runner.run_untagged = run_untagged;
        }
        if let Some(tags) = body
            .as_ref()
            .and_then(|b| b.get("tag_list"))
            .and_then(|v| v.as_array())
        {
            runner.tags = tags
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// GET /api/v4/runners/all
async fn list_all_runners(State(state): State<GitlabState>) -> impl IntoResponse {
    let s = state.lock().unwrap();
    let runners: Vec<Value> = s
        .runners
        .values()
        .map(|runner| {
            json!({
                "id": runner.id,
                "description": format!("mock-runner-{}", runner.id),
                "paused": runner.paused,
                "tag_list": runner.tags.clone(),
                "run_untagged": runner.run_untagged,
            })
        })
        .collect();
    Json(json!(runners)).into_response()
}

// DELETE /api/v4/runners/:id
async fn delete_runner(State(state): State<GitlabState>, Path(id): Path<i64>) -> StatusCode {
    let mut s = state.lock().unwrap();
    if s.runners.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// GET /api/v4/runners/:id/managers
async fn list_runner_managers(
    State(state): State<GitlabState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let s = state.lock().unwrap();
    if s.runners.contains_key(&id) {
        Json(json!([])).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// POST /api/v4/runners/:id/reset_authentication_token
async fn reset_runner_token(
    State(state): State<GitlabState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    if let Some(runner) = s.runners.get_mut(&id) {
        runner.token = format!("mock-new-token-{id}");
        let token = runner.token.clone();
        (StatusCode::CREATED, Json(json!({ "token": token }))).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// POST /api/v4/projects
async fn create_project(
    State(state): State<GitlabState>,
    body: Option<axum::extract::Json<Value>>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    let id = s.next_id();
    let name = body
        .as_ref()
        .and_then(|b| b.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("mock-project")
        .to_string();
    s.projects.insert(
        id,
        MockProject {
            id,
            name: name.clone(),
        },
    );
    let namespace = format!("mock-group/{name}");
    let web_url = format!("http://mock.gitlab.local/{namespace}");
    (
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": name,
            "path_with_namespace": namespace,
            "web_url": web_url,
        })),
    )
        .into_response()
}

// POST /api/v4/projects/:project_id/repository/branches
async fn create_branch(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    axum::extract::Json(req): axum::extract::Json<MockCreateBranchReq>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    if !s.projects.contains_key(&project_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let key = MockGitlabInner::branch_key(project_id, &req.branch);
    if s.branches.contains_key(&key) {
        return StatusCode::CONFLICT.into_response();
    }

    let sha = {
        let base_key = MockGitlabInner::branch_key(project_id, &req.ref_name);
        s.branches.get(&base_key).map(|branch| branch.sha.clone())
    }
    .unwrap_or_else(|| format!("branch-sha-{}", s.next_id()));
    s.set_branch_state(project_id, &req.branch, sha.clone(), None);
    s.add_pipeline_with_sha(
        project_id,
        &req.branch,
        "test_job",
        "success",
        "Branch created\n",
        &sha,
        None,
    );

    (
        StatusCode::CREATED,
        Json(json!({
            "name": req.branch,
            "ref": req.ref_name,
            "web_url": format!("http://mock.gitlab.local/project/{}/branches/{}", project_id, req.branch),
            "commit": { "id": sha },
        })),
    )
        .into_response()
}

// POST /api/v4/projects/:project_id/repository/commits
// Auto-creates a pipeline with one pending job when .gitlab-ci.yml is committed.
async fn commit_file(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    axum::extract::Json(req): axum::extract::Json<MockCommitReq>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    if !s.projects.contains_key(&project_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let branch = req.branch.unwrap_or_else(|| "main".to_string());
    let sha = format!("sha-{}", s.next_id());
    let ci_action = req
        .actions
        .iter()
        .find(|action| action.file_path.contains(".gitlab-ci.yml"));
    if let Some(action) = ci_action {
        let job_name = s
            .commit_job_name_override
            .clone()
            .or_else(|| {
                if action.content.contains("jeryu-test-run") {
                    Some("jeryu-test-run".to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "test_job".to_string());
        let status = s
            .commit_pipeline_status_override
            .clone()
            .unwrap_or_else(|| "pending".to_string());
        let yaml_errors = s.commit_yaml_errors_override.clone();
        s.set_branch_state(project_id, &branch, sha.clone(), Some(job_name.clone()));
        s.add_pipeline_with_sha(
            project_id,
            &branch,
            &job_name,
            &status,
            "God Mode Active\n",
            &sha,
            yaml_errors,
        );
    } else {
        s.set_branch_state(project_id, &branch, sha.clone(), None);
    }
    (
        StatusCode::CREATED,
        Json(json!({ "id": sha, "short_id": &sha[..8.min(sha.len())] })),
    )
        .into_response()
}

// POST /api/v4/projects/:project_id/ci/lint?include_merged_yaml=true
async fn lint_ci(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    axum::extract::Json(req): axum::extract::Json<MockLintReq>,
) -> impl IntoResponse {
    let s = state.lock().unwrap();
    if !s.projects.contains_key(&project_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(response) = &s.lint_response {
        return Json(json!({
            "valid": response.valid,
            "errors": response.errors,
            "warnings": response.warnings,
            "merged_yaml": response.merged_yaml,
        }))
        .into_response();
    }

    match validate_ci_yaml(&req.content) {
        Ok(merged_yaml) => Json(json!({
            "valid": true,
            "errors": [],
            "warnings": [],
            "merged_yaml": merged_yaml,
        }))
        .into_response(),
        Err(errors) => Json(json!({
            "valid": false,
            "errors": errors,
            "warnings": [],
            "merged_yaml": null,
        }))
        .into_response(),
    }
}

// GET /api/v4/projects/:project_id/pipelines
#[derive(Deserialize, Default)]
struct ListPipelinesQuery {
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    #[serde(rename = "per_page")]
    _per_page: Option<u32>,
}

async fn list_pipelines(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    Query(q): Query<ListPipelinesQuery>,
) -> impl IntoResponse {
    let s = state.lock().unwrap();
    let pipelines: Vec<Value> = s
        .pipelines
        .values()
        .filter(|p| p.project_id == project_id)
        .filter(|p| q.ref_name.as_ref().is_none_or(|r| p.ref_name == *r))
            .map(|p| {
                json!({
                    "id": p.id,
                    "sha": p.sha,
                    "ref": p.ref_name,
                    "status": p.status,
                    "yaml_errors": p.yaml_errors,
                    "web_url": format!("http://mock.gitlab.local/project/{}/pipelines/{}", project_id, p.id),
                })
            })
            .collect();
    Json(json!(pipelines)).into_response()
}

// GET /api/v4/projects/:project_id/pipelines/:pipeline_id
async fn get_pipeline(
    State(state): State<GitlabState>,
    Path((project_id, pipeline_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let s = state.lock().unwrap();
    let pipeline = match s.pipelines.get(&pipeline_id) {
        Some(pipeline) if pipeline.project_id == project_id => pipeline,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    Json(json!({
        "id": pipeline.id,
        "sha": pipeline.sha,
        "ref": pipeline.ref_name,
        "status": pipeline.status,
        "web_url": format!("http://mock.gitlab.local/project/{}/pipelines/{}", project_id, pipeline.id),
        "yaml_errors": pipeline.yaml_errors,
        "source": "push",
    }))
    .into_response()
}

// POST /api/v4/projects/:project_id/pipeline
async fn trigger_pipeline(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    axum::extract::Json(req): axum::extract::Json<Value>,
) -> impl IntoResponse {
    let ref_name = req
        .get("ref")
        .and_then(|value| value.as_str())
        .unwrap_or("main")
        .to_string();
    let mut s = state.lock().unwrap();
    if !s.projects.contains_key(&project_id) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let branch_key = MockGitlabInner::branch_key(project_id, &ref_name);
    let (sha, job_name) = match s.branches.get(&branch_key) {
        Some(branch) => (
            branch.sha.clone(),
            branch
                .ci_job_name
                .clone()
                .unwrap_or_else(|| "test_job".to_string()),
        ),
        None => {
            let sha = format!("trigger-sha-{}", s.next_id());
            (sha, "test_job".to_string())
        }
    };
    let status = s
        .commit_pipeline_status_override
        .clone()
        .unwrap_or_else(|| "pending".to_string());
    let yaml_errors = s.commit_yaml_errors_override.clone();
    let (pipeline_id, _) = s.add_pipeline_with_sha(
        project_id,
        &ref_name,
        &job_name,
        &status,
        "God Mode Active\n",
        &sha,
        yaml_errors,
    );

    Json(json!({ "id": pipeline_id })).into_response()
}

// POST /api/v4/projects/:project_id/pipelines/:pipeline_id/cancel
async fn cancel_pipeline(
    State(state): State<GitlabState>,
    Path((project_id, pipeline_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    let canceled = match s.pipelines.get_mut(&pipeline_id) {
        Some(pipeline) if pipeline.project_id == project_id => {
            pipeline.status = "canceled".to_string();
            true
        }
        _ => false,
    };
    if !canceled {
        return StatusCode::NOT_FOUND.into_response();
    }

    for job in s
        .jobs
        .values_mut()
        .filter(|job| job.pipeline_id == pipeline_id)
    {
        job.status = "canceled".to_string();
    }

    StatusCode::NO_CONTENT.into_response()
}

// DELETE /api/v4/projects/:project_id/repository/branches/:branch_name
async fn delete_branch(
    State(state): State<GitlabState>,
    Path((project_id, branch_name)): Path<(i64, String)>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    s.branches
        .remove(&MockGitlabInner::branch_key(project_id, &branch_name));
    StatusCode::NO_CONTENT.into_response()
}

fn validate_ci_yaml(content: &str) -> Result<String, Vec<String>> {
    let parsed: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(value) => value,
        Err(err) => {
            return Err(vec![format!("CI YAML failed to parse: {err}")]);
        }
    };

    let Some(root) = parsed.as_mapping() else {
        return Err(vec![
            "jobs config should contain at least one visible job".to_string(),
        ]);
    };

    let mut errors = Vec::new();
    let mut visible_job_count = 0usize;

    for (key, value) in root {
        let Some(job_name) = key.as_str() else {
            continue;
        };
        if job_name.starts_with('.') {
            continue;
        }

        let Some(job) = value.as_mapping() else {
            continue;
        };
        let Some(script) = job.get(&serde_yaml::Value::String("script".to_string())) else {
            continue;
        };

        visible_job_count += 1;
        match script {
            serde_yaml::Value::String(_) => {}
            serde_yaml::Value::Sequence(items) => {
                for (index, item) in items.iter().enumerate() {
                    if !matches!(item, serde_yaml::Value::String(_)) {
                        errors.push(format!(
                            "job `{job_name}` script item {index} must be a string, got {}",
                            yaml_value_kind(item)
                        ));
                    }
                }
            }
            other => errors.push(format!(
                "job `{job_name}` script must be a string or list of strings, got {}",
                yaml_value_kind(other)
            )),
        }
    }

    if visible_job_count == 0 {
        errors.push("jobs config should contain at least one visible job".to_string());
    }

    if errors.is_empty() {
        Ok(content.to_string())
    } else {
        Err(errors)
    }
}

fn yaml_value_kind(value: &serde_yaml::Value) -> &'static str {
    match value {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "bool",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "array",
        serde_yaml::Value::Mapping(_) => "object",
        serde_yaml::Value::Tagged(_) => "tagged",
    }
}

// GET /api/v4/projects/:project_id/pipelines/:pipeline_id/jobs
async fn list_pipeline_jobs(
    State(state): State<GitlabState>,
    Path((project_id, pipeline_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let _ = project_id;
    let s = state.lock().unwrap();
    let jobs: Vec<Value> = s
        .jobs
        .values()
        .filter(|j| j.pipeline_id == pipeline_id)
        .map(|j| {
            json!({
                "id": j.id,
                "name": j.name,
                "status": j.status,
                "stage": j.stage,
                "allow_failure": false,
                "pipeline": {
                    "id": j.pipeline_id,
                    "sha": null,
                    "ref": null,
                    "status": null,
                },
                "web_url": format!("http://mock.gitlab.local/jobs/{}", j.id),
            })
        })
        .collect();
    Json(json!(jobs)).into_response()
}

// GET /api/v4/projects/:project_id/jobs/:job_id/trace
async fn job_trace(
    State(state): State<GitlabState>,
    Path((_project_id, job_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let s = state.lock().unwrap();
    if let Some(job) = s.jobs.get(&job_id) {
        (StatusCode::OK, job.trace.clone()).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// POST /api/v4/projects/:project_id/jobs/:job_id/retry
async fn retry_job(
    State(state): State<GitlabState>,
    Path((project_id, job_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    let _ = project_id;
    let mut s = state.lock().unwrap();
    if let Some(job) = s.jobs.get_mut(&job_id) {
        job.status = "pending".to_string();
        let id = job.id;
        let name = job.name.clone();
        let stage = job.stage.clone();
        let pid = job.pipeline_id;
        // Also reset pipeline status.
        if let Some(p) = s.pipelines.get_mut(&pid) {
            p.status = "pending".to_string();
        }
        (
            StatusCode::CREATED,
            Json(json!({
                "id": id, "name": name, "status": "pending", "stage": stage,
                "allow_failure": false,
            })),
        )
            .into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

// GET /api/v4/projects/:project_id/pipelines/:pipeline_id/bridges
async fn list_pipeline_bridges(
    State(_state): State<GitlabState>,
    Path((_project_id, _pipeline_id)): Path<(i64, i64)>,
) -> impl IntoResponse {
    Json(json!([])).into_response()
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

fn build_router(state: GitlabState) -> Router {
    Router::new()
        // Health
        .route("/help", get(health_check))
        .route("/users/sign_in", get(health_check))
        // Runners
        .route("/api/v4/user/runners", post(create_runner))
        .route("/api/v4/runners/all", get(list_all_runners))
        .route(
            "/api/v4/runners/{id}",
            put(update_runner).delete(delete_runner),
        )
        .route("/api/v4/runners/{id}/managers", get(list_runner_managers))
        .route(
            "/api/v4/runners/{id}/reset_authentication_token",
            post(reset_runner_token),
        )
        // Projects
        .route("/api/v4/projects", post(create_project))
        .route(
            "/api/v4/projects/{project_id}/repository/branches",
            post(create_branch),
        )
        .route(
            "/api/v4/projects/{project_id}/repository/branches/{branch_name}",
            delete(delete_branch),
        )
        .route(
            "/api/v4/projects/{project_id}/repository/commits",
            post(commit_file),
        )
        .route("/api/v4/projects/{project_id}/ci/lint", post(lint_ci))
        // Pipelines
        .route(
            "/api/v4/projects/{project_id}/pipelines",
            get(list_pipelines),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}",
            get(get_pipeline),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}/jobs",
            get(list_pipeline_jobs),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}/bridges",
            get(list_pipeline_bridges),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines",
            post(trigger_pipeline),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}/cancel",
            post(cancel_pipeline),
        )
        // Jobs
        .route(
            "/api/v4/projects/{project_id}/jobs/{job_id}/trace",
            get(job_trace),
        )
        .route(
            "/api/v4/projects/{project_id}/jobs/{job_id}/retry",
            post(retry_job),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// MockGitlabServer public API
// ---------------------------------------------------------------------------

pub struct MockGitlabServer {
    pub state: GitlabState,
    pub base_url: String,
    _shutdown: tokio::sync::oneshot::Sender<()>,
}

impl MockGitlabServer {
    /// Start the mock server on an OS-assigned port.  Returns once the server
    /// is ready to accept connections.
    pub async fn start() -> Self {
        let state: GitlabState = Arc::new(Mutex::new(MockGitlabInner::default()));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock GitLab server");
        let port = listener
            .local_addr()
            .expect("mock server local addr")
            .port();
        let base_url = format!("http://127.0.0.1:{port}");

        let app = build_router(state.clone());

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("mock GitLab server error");
        });

        MockGitlabServer {
            state,
            base_url,
            _shutdown: shutdown_tx,
        }
    }

    /// Return a `GitlabClient` configured to talk to this mock server.
    pub fn gitlab_client(&self) -> jeryu::gitlab_client::GitlabClient {
        jeryu::gitlab_client::GitlabClient::new(&self.base_url, Some("mock-pat".to_string()))
    }

    // --- Convenience state mutators ------------------------------------------

    /// Advance all non-terminal jobs (and their parent pipelines) to `status`.
    pub fn advance_jobs(&self, status: &str) {
        self.state.lock().unwrap().advance_jobs(status);
    }

    /// Set a single job (and its pipeline) to `status`.
    pub fn set_job_status(&self, job_id: i64, status: &str) {
        self.state.lock().unwrap().set_job_status(job_id, status);
    }

    /// Return the current token for a registered runner, or None if not found.
    pub fn runner_token(&self, id: i64) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .runners
            .get(&id)
            .map(|r| r.token.clone())
    }

    /// Add a pre-configured pipeline+job to `project_id` and return `(pipeline_id, job_id)`.
    pub fn add_pipeline(
        &self,
        project_id: i64,
        ref_name: &str,
        job_name: &str,
        initial_status: &str,
        trace: &str,
    ) -> (i64, i64) {
        self.state.lock().unwrap().add_pipeline(
            project_id,
            ref_name,
            job_name,
            initial_status,
            trace,
        )
    }
}
