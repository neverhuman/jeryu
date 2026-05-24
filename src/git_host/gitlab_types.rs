use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct GitLabUser {
    pub(super) username: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabAuthor {
    pub(super) username: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabDiffRefs {
    pub(super) head_sha: Option<String>,
    pub(super) base_sha: Option<String>,
    pub(super) start_sha: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabChange {
    pub(super) old_path: Option<String>,
    pub(super) new_path: Option<String>,
    #[serde(default)]
    pub(super) diff: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabMergeRequest {
    pub(super) iid: i64,
    pub(super) title: String,
    pub(super) target_branch: String,
    #[serde(default)]
    pub(super) labels: Vec<String>,
    #[serde(default)]
    pub(super) draft: bool,
    #[serde(default)]
    pub(super) work_in_progress: bool,
    #[serde(default)]
    pub(super) sha: Option<String>,
    #[serde(default)]
    pub(super) diff_refs: Option<GitLabDiffRefs>,
    #[serde(default)]
    pub(super) author: Option<GitLabAuthor>,
    #[serde(default)]
    pub(super) changes: Option<Vec<GitLabChange>>,
}

#[derive(Debug, Serialize)]
pub(super) struct GitLabCommitStatusReq<'a> {
    pub(super) state: &'a str,
    #[serde(rename = "name")]
    pub(super) name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) target_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabCommitStatus {
    pub(super) id: i64,
    pub(super) target_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct GitLabNoteReq<'a> {
    pub(super) body: &'a str,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabNote {
    pub(super) id: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabApprovalResp {
    pub(super) id: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabTreeEntry {
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) kind: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabRepositoryFile {
    pub(super) content: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLabProtectedBranch {
    pub name: String,
}
