use super::gitlab_client_types::PipelineVariable;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct CreateProjectPatReq<'a> {
    pub(crate) name: &'a str,
    pub(crate) scopes: &'a [&'a str],
    pub(crate) access_level: i32,
    pub(crate) expires_at: &'a str,
}

#[derive(Serialize)]
pub(crate) struct CreateRunnerReq<'a> {
    pub(crate) description: &'a str,
    pub(crate) tag_list: &'a [&'a str],
    pub(crate) run_untagged: bool,
    pub(crate) runner_type: &'a str,
}

#[derive(Serialize)]
pub(crate) struct UpdateRunnerReq<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tag_list: Option<&'a [&'a str]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_untagged: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct SetPausedReq {
    pub(crate) paused: bool,
}

#[derive(Deserialize)]
pub(crate) struct ResetTokenResp {
    pub(crate) token: String,
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
pub(crate) struct ProtectBranchReq<'a> {
    pub(crate) name: &'a str,
    pub(crate) push_access_level: i32,
    pub(crate) merge_access_level: i32,
    pub(crate) allow_force_push: bool,
}

#[derive(Serialize)]
pub(crate) struct UpdateProjectPolicyReq<'a> {
    pub(crate) merge_method: &'a str,
    pub(crate) only_allow_merge_if_pipeline_succeeds: bool,
    pub(crate) allow_merge_on_skipped_pipeline: bool,
    pub(crate) only_allow_merge_if_all_discussions_are_resolved: bool,
    pub(crate) remove_source_branch_after_merge: bool,
    pub(crate) squash_option: &'a str,
}

#[derive(Serialize)]
pub(crate) struct CreateProjectReq<'a> {
    pub(crate) name: &'a str,
    pub(crate) visibility: &'a str,
    pub(crate) initialize_with_readme: bool,
}

#[derive(Serialize)]
pub(crate) struct LintCiReq<'a> {
    pub(crate) content: &'a str,
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

#[derive(Deserialize)]
pub(crate) struct PipelineResp {
    pub(crate) id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LintCiResp {
    pub(crate) valid: bool,
    pub(crate) errors: Vec<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) merged_yaml: Option<String>,
}
