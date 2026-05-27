use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct GitLabUser {
    pub(super) username: String,
}

// ── W-H-02 / W-H-03: project, branch, tag, tree, file, commit, compare DTOs ──
//
// Some fields are deserialized for forward-compat but not yet projected onto
// the JeRyu BFF surface. `dead_code` is silenced struct-wide rather than
// per-field so the GitLab schema stays self-documenting.

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabNamespace {
    pub(super) full_path: String,
    pub(super) kind: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabProject {
    pub(super) id: i64,
    pub(super) name: String,
    #[serde(default)]
    pub(super) path: String,
    pub(super) path_with_namespace: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) visibility: Option<String>,
    #[serde(default)]
    pub(super) default_branch: Option<String>,
    #[serde(default)]
    pub(super) archived: bool,
    #[serde(default)]
    pub(super) topics: Vec<String>,
    #[serde(default)]
    pub(super) tag_list: Vec<String>,
    #[serde(default)]
    pub(super) http_url_to_repo: Option<String>,
    #[serde(default)]
    pub(super) ssh_url_to_repo: Option<String>,
    #[serde(default)]
    pub(super) web_url: Option<String>,
    #[serde(default)]
    pub(super) forked_from_project: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) last_activity_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) namespace: Option<GitLabNamespace>,
}

#[derive(Debug, Serialize, Default)]
pub(super) struct CreateProjectReq<'a> {
    pub(super) name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) visibility: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) default_branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) initialize_with_readme: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) namespace_id: Option<i64>,
}

#[derive(Debug, Serialize, Default)]
pub(super) struct UpdateProjectReq<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) homepage: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) visibility: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) default_branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabBranchCommit {
    pub(super) id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabBranch {
    pub(super) name: String,
    pub(super) commit: GitLabBranchCommit,
    #[serde(default)]
    pub(super) protected: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabTag {
    pub(super) name: String,
    pub(super) commit: GitLabBranchCommit,
    #[serde(default)]
    pub(super) protected: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabRepoTreeEntry {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) path: String,
    #[serde(rename = "type")]
    pub(super) kind: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabRepoFile {
    pub(super) file_name: String,
    pub(super) file_path: String,
    pub(super) size: u64,
    pub(super) content: String,
    pub(super) encoding: String,
    pub(super) blob_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabCommit {
    pub(super) id: String,
    pub(super) message: String,
    #[serde(default)]
    pub(super) author_name: String,
    pub(super) committed_date: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub(super) parent_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabCompareDiff {
    #[serde(default)]
    pub(super) old_path: Option<String>,
    #[serde(default)]
    pub(super) new_path: Option<String>,
    #[serde(default)]
    pub(super) new_file: bool,
    #[serde(default)]
    pub(super) renamed_file: bool,
    #[serde(default)]
    pub(super) deleted_file: bool,
    #[serde(default)]
    pub(super) diff: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabCompareCommit {
    pub(super) id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GitLabCompare {
    #[serde(default)]
    pub(super) commit: Option<GitLabCompareCommit>,
    #[serde(default)]
    pub(super) commits: Vec<GitLabCompareCommit>,
    #[serde(default)]
    pub(super) diffs: Vec<GitLabCompareDiff>,
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

// ── W-H-04: MR discussions + reviews ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabDiscussionNoteAuthor {
    #[serde(default)]
    pub(super) username: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabDiscussionNotePosition {
    #[serde(default)]
    pub(super) new_path: Option<String>,
    #[serde(default)]
    pub(super) old_path: Option<String>,
    #[serde(default)]
    pub(super) new_line: Option<u32>,
    #[serde(default)]
    pub(super) old_line: Option<u32>,
    #[serde(default)]
    pub(super) base_sha: Option<String>,
    #[serde(default)]
    pub(super) head_sha: Option<String>,
    #[serde(default)]
    pub(super) start_sha: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabDiscussionNote {
    pub(super) id: i64,
    pub(super) body: String,
    #[serde(default)]
    pub(super) author: Option<GitLabDiscussionNoteAuthor>,
    pub(super) created_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub(super) resolvable: bool,
    #[serde(default)]
    pub(super) resolved: bool,
    #[serde(default)]
    pub(super) position: Option<GitLabDiscussionNotePosition>,
    #[serde(default)]
    pub(super) system: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabDiscussion {
    pub(super) id: String,
    #[serde(default)]
    pub(super) notes: Vec<GitLabDiscussionNote>,
}

#[derive(Debug, Serialize)]
pub(super) struct GitLabDiscussionCreatePosition<'a> {
    pub(super) position_type: &'static str,
    pub(super) base_sha: &'a str,
    pub(super) start_sha: &'a str,
    pub(super) head_sha: &'a str,
    pub(super) new_path: &'a str,
    pub(super) old_path: &'a str,
    pub(super) new_line: u32,
}

#[derive(Debug, Serialize)]
pub(super) struct GitLabCreateDiscussionReq<'a> {
    pub(super) body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) position: Option<GitLabDiscussionCreatePosition<'a>>,
}

#[derive(Debug, Serialize)]
pub(super) struct GitLabResolveDiscussionReq {
    pub(super) resolved: bool,
}

// ── W-H-05: MR merge with SHA ─────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(super) struct GitLabMergeMrReq<'a> {
    pub(super) sha: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) merge_commit_message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) squash_commit_message: Option<&'a str>,
    pub(super) squash: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabMergeMrResp {
    #[serde(default)]
    pub(super) merge_commit_sha: Option<String>,
    #[serde(default)]
    pub(super) squash_commit_sha: Option<String>,
    #[serde(default)]
    pub(super) sha: Option<String>,
    #[serde(default)]
    pub(super) state: Option<String>,
    #[serde(default)]
    pub(super) web_url: Option<String>,
}

// ── MR detail field surface for Merge Passport ─────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabMergeRequestDetail {
    pub(super) iid: i64,
    pub(super) title: String,
    pub(super) target_branch: String,
    pub(super) source_branch: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) labels: Vec<String>,
    #[serde(default)]
    pub(super) draft: bool,
    #[serde(default)]
    pub(super) work_in_progress: bool,
    #[serde(default)]
    pub(super) state: Option<String>,
    #[serde(default)]
    pub(super) sha: Option<String>,
    #[serde(default)]
    pub(super) diff_refs: Option<GitLabDiffRefs>,
    #[serde(default)]
    pub(super) author: Option<GitLabAuthor>,
    #[serde(default)]
    pub(super) merge_status: Option<String>,
    #[serde(default)]
    pub(super) detailed_merge_status: Option<String>,
    #[serde(default)]
    pub(super) has_conflicts: bool,
    #[serde(default)]
    pub(super) updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) web_url: Option<String>,
    #[serde(default)]
    pub(super) approvals_before_merge: Option<u32>,
}

// ── W-H-06: CI pipelines + jobs ────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabPipelineCi {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) sha: String,
    #[serde(default, rename = "ref")]
    pub(super) ref_name: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) source: Option<String>,
    #[serde(default)]
    pub(super) web_url: Option<String>,
    #[serde(default)]
    pub(super) created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabPipelineRef {
    #[serde(default)]
    pub(super) id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(super) struct GitLabJobCi {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) name: String,
    #[serde(default)]
    pub(super) stage: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub(super) duration: Option<f64>,
    #[serde(default)]
    pub(super) pipeline: Option<GitLabPipelineRef>,
}
