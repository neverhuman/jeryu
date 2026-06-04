//! Codegraph oracle REST adapter.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{error::Error, fmt};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_codegraph::{
    CodeGraphImpactPack, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore,
};
use serde_json::json;

use super::WebState;
use super::repositories::find_repo;

pub(super) async fn query_repo(
    State(state): State<std::sync::Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let query: CodeGraphQuery = match serde_json::from_slice(&body) {
        Ok(query) => query,
        Err(error) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_input",
                "codegraph query body failed validation",
                "validate codegraph query request",
                &format!("request body was not a valid CodeGraphQuery: {error}"),
                &[
                    "send JSON with ref and changed_paths fields",
                    "use changed_paths as repo-relative paths",
                ],
                "docs/errors.md#invalid-input",
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
            );
        }
    };

    match query_pack_for_repo(&state, &id, query) {
        Ok(pack) => Json(pack).into_response(),
        Err(error) => error.into_response(),
    }
}

pub(super) fn query_pack_for_repo(
    state: &WebState,
    id: &str,
    query: CodeGraphQuery,
) -> std::result::Result<CodeGraphImpactPack, CodeGraphRouteError> {
    let repo =
        find_repo(state, id).ok_or(CodeGraphRouteError::RepoNotFound { id: id.to_string() })?;
    let managed = state
        .repo_manager
        .open_parts(&repo.owner, &repo.name)
        .map_err(|err| CodeGraphRouteError::RepoNotFound {
            id: format!("{} ({err})", repo.full_name),
        })?;
    let git_bin = state.repo_manager.config().git_bin.clone();
    let commit = resolve_ref(&git_bin, &managed.path, &query.ref_name)?;
    let checkout = TempCheckout::new(&repo.full_name);
    materialize_checkout(&git_bin, &managed.path, &checkout.path, &commit)?;

    let store = CodeGraphStore::open(managed.path.join("jeryu").join("codegraph.sqlite"))
        .map_err(|err| CodeGraphRouteError::IndexFailed(err.to_string()))?;
    let service = CodeGraphService::new(checkout.path.clone(), store);
    service
        .query(
            CodeGraphRepoIdentity {
                id: repo.id.to_string(),
                owner: repo.owner.clone(),
                name: repo.name.clone(),
            },
            commit,
            query,
        )
        .map_err(|err| CodeGraphRouteError::IndexFailed(err.to_string()))
}

#[derive(Debug)]
pub(super) enum CodeGraphRouteError {
    RepoNotFound { id: String },
    InvalidRef { ref_name: String, reason: String },
    MaterializeFailed(String),
    IndexFailed(String),
}

impl CodeGraphRouteError {
    fn into_response(self) -> AxumResponse {
        match self {
            Self::RepoNotFound { id } => error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                "repository not found",
                "load repository for codegraph query",
                &format!("repository {id} is not registered or lacks a materialized bare repo"),
                &[
                    "verify the repository id or owner/name pair",
                    "import or create the repository before querying codegraph",
                ],
                "docs/errors.md#not-found",
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
            ),
            Self::InvalidRef { ref_name, reason } => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_ref",
                "repository ref could not be resolved",
                "resolve codegraph query ref to a commit",
                &format!("ref {ref_name} did not resolve to a commit: {reason}"),
                &[
                    "use a branch, tag, or commit reachable from the managed repository",
                    "refresh the local import before retrying",
                ],
                "docs/errors.md#invalid-input",
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph_invalid_ref",
            ),
            Self::MaterializeFailed(reason) => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_materialize_failed",
                "repository checkout could not be materialized",
                "materialize isolated checkout for codegraph indexing",
                &reason,
                &[
                    "verify git is installed and the bare repo is healthy",
                    "retry after importing the repository into Jeryu git storage",
                ],
                "docs/errors.md#invalid-input",
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
            ),
            Self::IndexFailed(reason) => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_index_failed",
                "codegraph index failed",
                "build Rust/Cargo codegraph impact pack",
                &reason,
                &[
                    "verify the repository has a Cargo workspace for v1 Rust/Cargo indexing",
                    "inspect malformed Jankurai governance files before retrying",
                ],
                "docs/errors.md#invalid-input",
                "rerun cargo test -p jeryu-codegraph --jobs 40",
            ),
        }
    }
}

impl fmt::Display for CodeGraphRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepoNotFound { id } => write!(f, "repository not found: {id}"),
            Self::InvalidRef { ref_name, reason } => {
                write!(f, "ref {ref_name} did not resolve to a commit: {reason}")
            }
            Self::MaterializeFailed(reason) => write!(f, "materialize checkout failed: {reason}"),
            Self::IndexFailed(reason) => write!(f, "codegraph index failed: {reason}"),
        }
    }
}

impl Error for CodeGraphRouteError {}

fn resolve_ref(
    git_bin: &str,
    bare_repo: &Path,
    ref_name: &str,
) -> std::result::Result<String, CodeGraphRouteError> {
    let git_dir = format!("--git-dir={}", bare_repo.display());
    let rev = format!("{ref_name}^{{commit}}");
    run_git(git_bin, &[&git_dir, "rev-parse", "--verify", &rev], None).map_err(|reason| {
        CodeGraphRouteError::InvalidRef {
            ref_name: ref_name.to_string(),
            reason,
        }
    })
}

fn materialize_checkout(
    git_bin: &str,
    bare_repo: &Path,
    checkout: &Path,
    commit: &str,
) -> std::result::Result<(), CodeGraphRouteError> {
    let bare = bare_repo.display().to_string();
    let checkout_path = checkout.display().to_string();
    run_git(
        git_bin,
        &["clone", "--quiet", "--no-checkout", &bare, &checkout_path],
        None,
    )
    .map_err(CodeGraphRouteError::MaterializeFailed)?;
    run_git(
        git_bin,
        &["-C", &checkout_path, "checkout", "--quiet", commit],
        None,
    )
    .map_err(CodeGraphRouteError::MaterializeFailed)?;
    Ok(())
}

fn run_git(
    git_bin: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> std::result::Result<String, String> {
    let mut command = Command::new(git_bin);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .output()
        .map_err(|err| format!("git invocation failed: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct TempCheckout {
    path: PathBuf,
}

impl TempCheckout {
    fn new(repo_id: &str) -> Self {
        let safe_repo = repo_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' {
                    ch
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let path = std::env::temp_dir().join(format!(
            "jeryu-codegraph-{safe_repo}-{}-{}",
            std::process::id(),
            epoch_nanos()
        ));
        Self { path }
    }
}

impl Drop for TempCheckout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    purpose: &str,
    reason: &str,
    common_fixes: &[&str],
    docs_url: &str,
    repair_hint: &str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "code": code,
            "message": message,
            "jeryu_repair_hint": {
                "purpose": purpose,
                "reason": reason,
                "common_fixes": common_fixes,
                "docs_url": docs_url,
                "repair_hint": repair_hint,
            }
        })),
    )
        .into_response()
}
