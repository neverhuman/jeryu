//! Codegraph oracle REST facade.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{error::Error, fmt};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_codegraph::{
    CodeGraphImpactPack, CodeGraphQuery, CodeGraphRepoIdentity, CodeGraphService, CodeGraphStore,
    CodegraphQuery, query_store,
};
use serde_json::{Value, json};

use super::WebState;
use super::repositories::find_repo;

pub(super) fn query_compat_pack_for_repo(
    state: &Arc<WebState>,
    id: &str,
    request: CodegraphQuery,
) -> Result<jeryu_codegraph::CodegraphImpactPack, String> {
    if find_repo(state, id).is_none() {
        return Err(format!("repository {id} not found for codegraph query"));
    }
    query_store(&state.codegraph_store, &request).map_err(|err| err.to_string())
}

pub(super) async fn query(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    if find_repo(&state, &id).is_none() {
        return codegraph_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "query repository codegraph",
            "repository not found for codegraph query",
            &[
                "verify the repository id or owner/name pair",
                "refresh the local forge import before retrying",
            ],
            "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
        );
    }
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_invalid_request",
                "query repository codegraph",
                &err.to_string(),
                &[
                    "send a JSON object matching the codegraph query contract",
                    "use changed paths as repo-relative strings",
                ],
                "fix the request body, then rerun the codegraph API proof lane",
            );
        }
    };

    if is_rich_query(&value) {
        let request: CodeGraphQuery = match serde_json::from_value(value) {
            Ok(request) => request,
            Err(err) => {
                return codegraph_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "codegraph_invalid_request",
                    "query repository codegraph",
                    &err.to_string(),
                    &[
                        "send ref and changed_paths fields for the rich codegraph oracle",
                        "use changed_paths as repo-relative paths",
                    ],
                    "fix the request body, then rerun the codegraph API proof lane",
                );
            }
        };
        return match query_pack_for_repo(&state, &id, request) {
            Ok(pack) => Json(pack).into_response(),
            Err(error) => error.into_response(),
        };
    }

    let request: CodegraphQuery = match serde_json::from_value(value) {
        Ok(request) => request,
        Err(err) => {
            return codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_invalid_request",
                "query repository codegraph",
                &err.to_string(),
                &[
                    "send changed_paths as an array of repo-relative strings",
                    "send symbol and crate_name as strings when filtering the oracle pack",
                ],
                "fix the request body, then rerun the codegraph API proof lane",
            );
        }
    };
    match query_compat_pack_for_repo(&state, &id, request) {
        Ok(pack) => Json(pack).into_response(),
        Err(err) => codegraph_error(
            StatusCode::FAILED_DEPENDENCY,
            "codegraph_query_failed",
            "query repository codegraph",
            &err,
            &[
                "rerun jeryu-codegraph index before querying",
                "inspect the auxiliary codegraph SQLite store",
            ],
            "rerun cargo test -p jeryu-codegraph -p jeryu-mcp --jobs 40 code",
        ),
    }
}

fn is_rich_query(value: &Value) -> bool {
    ["ref", "intent", "question", "max_tokens"]
        .iter()
        .any(|key| value.get(*key).is_some())
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
            Self::RepoNotFound { id } => codegraph_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "load repository for codegraph query",
                &format!("repository {id} is not registered or lacks a materialized bare repo"),
                &[
                    "verify the repository id or owner/name pair",
                    "import or create the repository before querying codegraph",
                ],
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
            ),
            Self::InvalidRef { ref_name, reason } => codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_ref",
                "resolve codegraph query ref to a commit",
                &format!("ref {ref_name} did not resolve to a commit: {reason}"),
                &[
                    "use a branch, tag, or commit reachable from the managed repository",
                    "refresh the local import before retrying",
                ],
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph_invalid_ref",
            ),
            Self::MaterializeFailed(reason) => codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_materialize_failed",
                "materialize isolated checkout for codegraph indexing",
                &reason,
                &[
                    "verify git is installed and the bare repo is healthy",
                    "retry after importing the repository into Jeryu git storage",
                ],
                "rerun cargo test -p jeryu-api --features web --jobs 40 codegraph",
            ),
            Self::IndexFailed(reason) => codegraph_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "codegraph_index_failed",
                "build Rust/Cargo codegraph impact pack",
                &reason,
                &[
                    "verify the repository has a Cargo workspace for v1 Rust/Cargo indexing",
                    "inspect malformed Jankurai governance files before retrying",
                ],
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

fn codegraph_error(
    status: StatusCode,
    code: &'static str,
    purpose: &'static str,
    reason: &str,
    common_fixes: &'static [&'static str],
    repair_hint: &'static str,
) -> AxumResponse {
    (
        status,
        Json(json!({
            "code": code,
            "message": reason,
            "purpose": purpose,
            "reason": reason,
            "common_fixes": common_fixes,
            "docs_url": "docs/errors.md#not-found",
            "repair_hint": repair_hint,
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use serde_json::{Value, json};

    use super::{CodeGraphRouteError, TempCheckout, codegraph_error, is_rich_query, run_git};
    use axum::http::StatusCode;

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[test]
    fn rich_query_detection_is_additive() {
        assert!(!is_rich_query(&json!({ "changed_paths": ["src/lib.rs"] })));
        assert!(is_rich_query(&json!({ "ref": "main" })));
        assert!(is_rich_query(&json!({ "intent": "audit" })));
        assert!(is_rich_query(&json!({ "question": "what should I read?" })));
        assert!(is_rich_query(&json!({ "max_tokens": 4096 })));
    }

    #[test]
    fn git_wrapper_reports_success_status_failure_and_spawn_failure() {
        let version = run_git("git", &["--version"], None).expect("git version");
        assert!(version.starts_with("git version"));
        let status_error = run_git("git", &["rev-parse", "--verify", "missing-ref"], None)
            .expect_err("missing ref should fail");
        assert!(!status_error.is_empty());
        let spawn_error = run_git("jeryu-missing-git-binary", &["--version"], None)
            .expect_err("missing binary should fail");
        assert!(spawn_error.contains("git invocation failed"));
    }

    #[test]
    fn route_error_display_strings_are_operator_readable() {
        assert_eq!(
            CodeGraphRouteError::RepoNotFound {
                id: "repo-1".to_string()
            }
            .to_string(),
            "repository not found: repo-1"
        );
        assert!(
            CodeGraphRouteError::InvalidRef {
                ref_name: "main".to_string(),
                reason: "missing".to_string()
            }
            .to_string()
            .contains("main")
        );
        assert!(
            CodeGraphRouteError::MaterializeFailed("clone failed".to_string())
                .to_string()
                .contains("clone failed")
        );
        assert!(
            CodeGraphRouteError::IndexFailed("no Cargo.toml".to_string())
                .to_string()
                .contains("no Cargo.toml")
        );
    }

    #[tokio::test]
    async fn route_errors_render_typed_json_bodies() {
        let errors = [
            (
                CodeGraphRouteError::RepoNotFound {
                    id: "repo-1".to_string(),
                },
                "not_found",
            ),
            (
                CodeGraphRouteError::InvalidRef {
                    ref_name: "main".to_string(),
                    reason: "missing".to_string(),
                },
                "invalid_ref",
            ),
            (
                CodeGraphRouteError::MaterializeFailed("clone failed".to_string()),
                "codegraph_materialize_failed",
            ),
            (
                CodeGraphRouteError::IndexFailed("no Cargo.toml".to_string()),
                "codegraph_index_failed",
            ),
        ];
        for (error, code) in errors {
            let body = response_json(error.into_response()).await;
            assert_eq!(body["code"], code);
            assert_eq!(body["docs_url"], "docs/errors.md#not-found");
        }

        let body = response_json(codegraph_error(
            StatusCode::FAILED_DEPENDENCY,
            "codegraph_query_failed",
            "query codegraph",
            "storage unavailable",
            &["rerun index"],
            "rerun focused proof",
        ))
        .await;
        assert_eq!(body["code"], "codegraph_query_failed");
        assert_eq!(body["common_fixes"], json!(["rerun index"]));
    }

    #[test]
    fn temp_checkout_path_sanitizes_repo_identity() {
        let checkout = TempCheckout::new("alice/repo.name");
        let path = checkout.path.to_string_lossy();
        assert!(path.contains("alice-repo-name"));
    }
}
