use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct CreateProjectPatReq<'a> {
    pub(crate) name: &'a str,
    pub(crate) scopes: &'a [&'a str],
    pub(crate) access_level: i32,
    pub(crate) expires_at: &'a str,
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
pub(crate) struct CreateRunnerReq<'a> {
    pub(crate) description: &'a str,
    pub(crate) tag_list: &'a [&'a str],
    pub(crate) run_untagged: bool,
    pub(crate) runner_type: &'a str,
}

#[derive(Serialize)]
pub(crate) struct SetPausedReq {
    pub(crate) paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct RunnerManager {
    pub system_id: Option<String>,
    pub status: Option<String>,
    pub contacted_at: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ResetTokenResp {
    pub(crate) token: String,
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
    #[serde(default)]
    pub pipeline: Option<PipelineRef>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_duration: Option<f64>,
    pub duration: Option<f64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub runner: Option<JobRunner>,
}

impl Job {
    pub fn effective_pipeline_id(&self) -> Option<i64> {
        match self.pipeline_id {
            Some(id) => Some(id),
            None => self.pipeline.as_ref().map(|p| p.id),
        }
    }
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
pub struct PipelineVariableValue {
    pub key: String,
    pub value: String,
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
pub(crate) struct CreateWebhookReq<'a> {
    pub(crate) url: &'a str,
    pub(crate) token: &'a str,
    pub(crate) job_events: bool,
    pub(crate) pipeline_events: bool,
    pub(crate) push_events: bool,
    pub(crate) merge_requests_events: bool,
}

#[derive(Deserialize)]
pub(crate) struct WebhookResp {
    pub(crate) id: i64,
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
pub(crate) struct CreateIssueReq<'a> {
    pub(crate) title: &'a str,
    pub(crate) description: &'a str,
    pub(crate) labels: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) assignee_ids: Option<Vec<i64>>,
}

#[derive(Serialize)]
pub(crate) struct UpdateLabelsReq {
    pub(crate) labels: String,
}

#[derive(Serialize)]
pub(crate) struct NoteReq<'a> {
    pub(crate) body: &'a str,
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
pub(crate) struct CreateMrReq<'a> {
    pub(crate) source_branch: &'a str,
    pub(crate) target_branch: &'a str,
    pub(crate) title: &'a str,
    pub(crate) description: &'a str,
    pub(crate) remove_source_branch: bool,
}

#[derive(Serialize)]
pub(crate) struct CreateBranchReq<'a> {
    pub(crate) branch: &'a str,
    #[serde(rename = "ref")]
    pub(crate) ref_name: &'a str,
}

#[derive(Serialize)]
pub(crate) struct CreateProjectReq<'a> {
    pub(crate) name: &'a str,
    pub(crate) visibility: &'a str,
    pub(crate) initialize_with_readme: bool,
}

#[derive(Serialize)]
pub(crate) struct CommitAction<'a> {
    pub(crate) action: &'a str,
    pub(crate) file_path: &'a str,
    pub(crate) content: &'a str,
}

#[derive(Serialize)]
pub(crate) struct CreateCommitReq<'a> {
    pub(crate) branch: &'a str,
    pub(crate) commit_message: &'a str,
    pub(crate) actions: Vec<CommitAction<'a>>,
}

#[derive(Deserialize)]
pub(crate) struct CreateCommitResp {
    pub(crate) id: String,
}

#[derive(Serialize)]
pub(crate) struct CreatePipelineReq<'a> {
    #[serde(rename = "ref")]
    pub(crate) ref_name: &'a str,
    pub(crate) variables: Vec<PipelineVariable<'a>>,
}

#[derive(Serialize)]
pub struct PipelineVariable<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Deserialize)]
pub(crate) struct PipelineResp {
    pub(crate) id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_pipeline_id_prefers_nested_pipeline_metadata() {
        let job: Job = serde_json::from_str(
            r#"{
                "id": 17,
                "name": "build-enclave-server",
                "status": "running",
                "stage": "package",
                "allow_failure": false,
                "pipeline": { "id": 4242, "status": "running" }
            }"#,
        )
        .expect("deserialize job");

        assert_eq!(job.pipeline_id, None);
        assert_eq!(job.effective_pipeline_id(), Some(4242));
    }
}
