use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{CreatePullRequestRequest, ForgeError};
use serde::{Deserialize, Serialize};

use crate::web::WebState;
use crate::web::workcells_support::{
    TypedError, forge_error, manager, parse_json_body, typed_error, workcell_error,
    workcell_not_found,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportRepairPrRequest {
    workcell_id: String,
    runner_epoch: u64,
    branch_suffix: String,
    #[serde(default)]
    changed_files: Vec<String>,
    owner: String,
    repo: String,
    author: String,
    #[serde(default)]
    target_branch: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportRepairPrResponse {
    workcell_id: String,
    branch: String,
    target_branch: String,
    pull_request_number: u64,
}

pub(in crate::web) async fn export_pr(
    State(state): State<Arc<WebState>>,
    AxumPath(workcell_id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let request: ExportRepairPrRequest = match parse_json_body(
        &body,
        "export a repair branch into a pull request",
        "rerun cargo test -p jeryu-api --features web --jobs 40",
    ) {
        Ok(request) => request,
        Err(response) => return *response,
    };
    if request.workcell_id != workcell_id {
        return typed_error(TypedError {
            status: StatusCode::BAD_REQUEST,
            code: "workcell_id_mismatch",
            purpose: "export a repair branch into a pull request",
            reason: "request path and body disagreed on the workcell id",
            common_fixes: &[
                "send the same workcell id in the path and request body",
                "reload the workcell status before retrying the export",
            ],
            docs_url: "docs/testing.md#workcells",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
            message: "the request body did not match the selected workcell",
        });
    }

    let ExportRepairPrRequest {
        workcell_id: _,
        runner_epoch,
        branch_suffix,
        changed_files: _,
        owner,
        repo,
        author,
        target_branch,
        title,
        body,
    } = request;

    let mut manager = manager(&state);
    let branch = match manager.export_repair_branch(&workcell_id, runner_epoch, branch_suffix) {
        Ok(branch) => branch,
        Err(err) => return workcell_error(err),
    };

    let lease = match manager.workcell(&workcell_id).cloned() {
        Some(lease) => lease,
        None => return workcell_not_found(&workcell_id),
    };
    let target_branch = target_branch
        .or_else(|| lease.startup_main_ref.clone())
        .map(normalize_pr_base)
        .unwrap_or_else(|| "main".to_string());
    let title = title.unwrap_or_else(|| format!("Repair {}", lease.workcell_id));
    let body = body.or_else(|| {
        Some(format!(
            "Workcell: {}\nFailure log: {}\n",
            lease.workcell_id,
            lease
                .failure_log_digest
                .clone()
                .or_else(|| lease
                    .frozen_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.failure_log_digest.clone()))
                .unwrap_or_else(|| "unknown".to_string())
        ))
    });
    let snapshot = lease.frozen_snapshot.as_ref();
    let head_sha = snapshot
        .map(|snapshot| snapshot.head_sha.clone())
        .or_else(|| lease.startup_head_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let base_sha = snapshot
        .map(|snapshot| snapshot.base_sha.clone())
        .or_else(|| lease.startup_base_sha.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let allowed_prefixes = derive_allowed_prefixes(&lease.allowed_paths, &lease.workspace_root);
    let bare_repo = match state.repo_manager.resolve_parts(&owner, &repo) {
        Ok(repository) => repository.path,
        Err(err) => return forge_error(ForgeError::Storage(err.to_string())),
    };
    let git_bin = state.repo_manager.config().git_bin.clone();
    let changed_files = match jeryu_codegraph::enforce_export_slice(
        &base_sha,
        &head_sha,
        &git_bin,
        &bare_repo,
        &allowed_prefixes,
    ) {
        Ok(files) => files,
        Err(denied) => {
            let message = match denied.git_error {
                Some(git_error) => {
                    format!("the export slice gate could not verify the diff: {git_error}")
                }
                None => format!(
                    "the export changed files outside the workcell slice: {}",
                    denied.out_of_slice_paths.join(", ")
                ),
            };
            return typed_error(TypedError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "workcell_export_slice_denied",
                purpose: "export a repair branch into a pull request",
                reason: &message,
                common_fixes: &[
                    "restrict the repair to files inside the workcell's allowed paths",
                    "reclaim the workcell with a lease that covers the changed files",
                ],
                docs_url: "docs/testing.md#workcells",
                repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
                message: &message,
            });
        }
    };
    let pr = match state.github.core().create_pull_request(
        &owner,
        &repo,
        &author,
        CreatePullRequestRequest {
            title,
            body,
            head: branch.clone(),
            base: target_branch.clone(),
            head_sha: Some(head_sha),
            base_sha: Some(base_sha),
            source_repository: Some(format!("{owner}/{repo}")),
            draft: false,
            commits: Vec::new(),
            changed_files,
        },
    ) {
        Ok(pr) => pr,
        Err(err) => return forge_error(err),
    };

    (
        StatusCode::CREATED,
        Json(ExportRepairPrResponse {
            workcell_id,
            branch,
            target_branch,
            pull_request_number: pr.number,
        }),
    )
        .into_response()
}

/// Derives repo-relative export-slice prefixes from a lease's absolute
/// `allowed_paths`, anchored at the repo checkout root.
fn derive_allowed_prefixes(allowed_paths: &[PathBuf], workspace_root: &Path) -> Vec<String> {
    let prefixes: Vec<String> = allowed_paths
        .iter()
        .filter_map(|path| path.strip_prefix(workspace_root).ok())
        .map(|relative| relative.to_string_lossy().to_string())
        .collect();
    let has_specific = prefixes.iter().any(|prefix| !prefix.is_empty());
    if has_specific {
        prefixes
            .into_iter()
            .filter(|prefix| !prefix.is_empty())
            .collect()
    } else {
        prefixes
    }
}

fn normalize_pr_base(ref_name: String) -> String {
    let without_heads = ref_name
        .strip_prefix("refs/heads/")
        .unwrap_or(ref_name.as_str());
    without_heads
        .strip_prefix("origin/")
        .unwrap_or(without_heads)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_pr_base;

    #[test]
    fn normalize_pr_base_strips_heads_and_origin_only() {
        assert_eq!(normalize_pr_base("refs/heads/main".into()), "main");
        assert_eq!(normalize_pr_base("origin/main".into()), "main");
        assert_eq!(
            normalize_pr_base("origin/release/2026".into()),
            "release/2026"
        );
        assert_eq!(
            normalize_pr_base("feature/workcell".into()),
            "feature/workcell"
        );
    }
}
