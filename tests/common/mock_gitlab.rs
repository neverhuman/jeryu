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
    routing::{get, post, put},
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

#[derive(Default, Debug)]
pub struct MockGitlabInner {
    next_id: i64,
    pub runners: HashMap<i64, MockRunner>,
    pub projects: HashMap<i64, MockProject>,
    pub pipelines: HashMap<i64, MockPipeline>,
    pub jobs: HashMap<i64, MockJob>,
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
        let pid = self.next_id();
        let jid = self.next_id();
        self.pipelines.insert(
            pid,
            MockPipeline {
                id: pid,
                project_id,
                ref_name: ref_name.to_string(),
                status: initial_status.to_string(),
                sha: format!("deadbeef{pid:08x}"),
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
    s.runners.insert(
        id,
        MockRunner {
            id,
            token: token.clone(),
            paused: false,
            tags,
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
        if let Some(paused) = body.as_ref().and_then(|b| b.get("paused")).and_then(|v| v.as_bool()) {
            runner.paused = paused;
        }
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

// DELETE /api/v4/runners/:id
async fn delete_runner(
    State(state): State<GitlabState>,
    Path(id): Path<i64>,
) -> StatusCode {
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
    s.projects.insert(id, MockProject { id, name: name.clone() });
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

// POST /api/v4/projects/:project_id/repository/commits
// Auto-creates a pipeline with one pending job when .gitlab-ci.yml is committed.
async fn commit_file(
    State(state): State<GitlabState>,
    Path(project_id): Path<i64>,
    body: Option<axum::extract::Json<Value>>,
) -> impl IntoResponse {
    let mut s = state.lock().unwrap();
    if !s.projects.contains_key(&project_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Detect a CI file commit — create a pipeline automatically.
    let is_ci_file = body
        .as_ref()
        .and_then(|b| b.get("actions"))
        .and_then(|v| v.as_array())
        .map(|actions| {
            actions.iter().any(|a| {
                a.get("file_path")
                    .and_then(|p| p.as_str())
                    .is_some_and(|p| p.contains(".gitlab-ci.yml"))
            })
        })
        .unwrap_or(false);

    let sha = format!("sha-{}", s.next_id());
    if is_ci_file {
        s.add_pipeline(project_id, "main", "test_job", "pending", "God Mode Active\n");
    }
    (
        StatusCode::CREATED,
        Json(json!({ "id": sha, "short_id": &sha[..8.min(sha.len())] })),
    )
        .into_response()
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
        .filter(|p| q.ref_name.as_deref().map_or(true, |r| p.ref_name == r))
        .map(|p| {
            json!({
                "id": p.id,
                "sha": p.sha,
                "ref": p.ref_name,
                "status": p.status,
                "web_url": format!("http://mock.gitlab.local/project/{}/pipelines/{}", project_id, p.id),
            })
        })
        .collect();
    Json(json!(pipelines)).into_response()
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
            "/api/v4/projects/{project_id}/repository/commits",
            post(commit_file),
        )
        // Pipelines
        .route(
            "/api/v4/projects/{project_id}/pipelines",
            get(list_pipelines),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}/jobs",
            get(list_pipeline_jobs),
        )
        .route(
            "/api/v4/projects/{project_id}/pipelines/{pipeline_id}/bridges",
            get(list_pipeline_bridges),
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
        self.state
            .lock()
            .unwrap()
            .add_pipeline(project_id, ref_name, job_name, initial_status, trace)
    }
}
