//! Owner: GitLab REST Client subsystem
//! Proof: `cargo nextest run -p jeryu -- gitlab_client`
//! Invariants: HTTP calls preserve GitLab semantics, redact tokens, and surface status-specific failures.
//! GitLab REST API client for jeryu.
//!
//! Thin, purpose-built wrapper around reqwest. Every method maps to
//! one GitLab REST endpoint. No magic.

use anyhow::{Context, Result};
use reqwest::{Client, Method};

#[path = "gitlab_client_types.rs"]
mod gitlab_client_types;
#[path = "gitlab_client_types_requests.rs"]
mod gitlab_client_types_requests;
pub use gitlab_client_types::{
    Issue, Job, JobRunner, MergeRequest, Pipeline, PipelineBridge, PipelineRef, PipelineVariable,
    PipelineVariableValue, Project, ProjectPatResp, RunnerCreated, RunnerInfo, RunnerManager,
};
use gitlab_client_types_requests::{
    CommitAction, CreateBranchReq, CreateCommitReq, CreateCommitResp, CreateIssueReq, CreateMrReq,
    CreatePipelineReq, CreateProjectPatReq, CreateProjectReq, CreateRunnerReq, CreateWebhookReq,
    LintCiReq, LintCiResp, NoteReq, PipelineResp, ProtectBranchReq, ResetTokenResp, SetPausedReq,
    UpdateLabelsReq, UpdateProjectPolicyReq, UpdateRunnerReq, WebhookResp,
};

#[path = "gitlab_client_branches.rs"]
mod gitlab_client_branches;
#[path = "gitlab_client_issues.rs"]
mod gitlab_client_issues;
#[path = "gitlab_client_jobs.rs"]
mod gitlab_client_jobs;
#[path = "gitlab_client_merge_requests.rs"]
mod gitlab_client_merge_requests;
#[path = "gitlab_client_pipelines.rs"]
mod gitlab_client_pipelines;
#[path = "gitlab_client_projects.rs"]
mod gitlab_client_projects;
#[path = "gitlab_client_runners.rs"]
mod gitlab_client_runners;
#[path = "gitlab_client_tls.rs"]
mod gitlab_client_tls;
#[path = "gitlab_client_webhooks.rs"]
mod gitlab_client_webhooks;

#[path = "gitlab_client_core.rs"]
mod gitlab_client_core;

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitlabClient {
    base_url: String,
    client: Client,
    pat: Option<String>,
}
