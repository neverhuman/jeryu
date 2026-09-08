//! Authenticated repository creation for the browser's preview/execute contract.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use axum::Json;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use jeryu_core::{AccountSummary, UserRole};
use jeryu_gitd::RepoId;
use jeryu_readmodel::contracts::{
    CreateRepositoryPreview, CreateRepositoryRequest, RepositoryVisibility,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{WebState, api_error, repositories::repo_summary};

#[cfg(test)]
mod tests;

fn validate(
    state: &WebState,
    account: &AccountSummary,
    request: &CreateRepositoryRequest,
) -> Result<()> {
    ensure!(
        account.role == UserRole::Admin || account.login == request.owner,
        "repositories may only be created in your own namespace"
    );
    RepoId::new(&request.owner, &request.name)?;
    ensure!(
        !request.name.ends_with(".git"),
        "name must omit the .git suffix"
    );
    ensure!(
        request.host == "jeryu",
        "only the jeryu host supports repository creation"
    );
    ensure!(
        matches!(
            request.visibility,
            RepositoryVisibility::Private | RepositoryVisibility::Public
        ),
        "choose public or private visibility"
    );
    ensure!(
        request.topics.is_empty()
            && request.template.is_none()
            && request.gitignore_template.is_none()
            && request.license_template.is_none(),
        "repository topics and templates are not supported by this server"
    );
    ensure!(
        request
            .family
            .as_ref()
            .is_none_or(|family| !family.trim().is_empty()),
        "family must not be blank"
    );
    let branch = request.default_branch.as_deref().unwrap_or("main");
    ensure!(!branch.starts_with('-'), "invalid default branch");
    let output = git_command(state)
        .args(["check-ref-format", &format!("refs/heads/{branch}")])
        .output()?;
    ensure!(output.status.success(), "invalid default branch");
    Ok(())
}

pub(super) async fn preview(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    Json(request): Json<CreateRepositoryRequest>,
) -> Response {
    if account.role != UserRole::Admin && account.login != request.owner {
        return api_error(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "repository owner must match the authenticated account",
        );
    }
    if let Err(error) = validate(&state, &account, &request) {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            &error.to_string(),
        );
    }
    if !request.dry_run {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "preview requires dry_run=true",
        );
    }
    if state
        .core
        .get_repository(&request.owner, &request.name)
        .is_ok()
    {
        return api_error(
            StatusCode::CONFLICT,
            "already_exists",
            "repository already exists",
        );
    }
    Json(CreateRepositoryPreview {
        normalized_name: request.name,
        target_owner: request.owner,
        visibility: request.visibility,
        initial_files: if request.initialize_readme {
            vec!["README.md".into()]
        } else {
            vec![]
        },
        settings_to_apply: vec![format!(
            "Default branch: {}",
            request.default_branch.as_deref().unwrap_or("main")
        )],
        side_effects: vec!["Create a durable repository and managed Git storage".into()],
        warnings: vec![],
    })
    .into_response()
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    request_sha256: String,
    repository_id: Option<String>,
}

pub(super) async fn create(
    State(state): State<Arc<WebState>>,
    Extension(account): Extension<AccountSummary>,
    headers: HeaderMap,
    Json(request): Json<CreateRepositoryRequest>,
) -> Response {
    if account.role != UserRole::Admin && account.login != request.owner {
        return api_error(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "repository owner must match the authenticated account",
        );
    }
    if let Err(error) = validate(&state, &account, &request) {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            &error.to_string(),
        );
    }
    if request.dry_run {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "use the preview endpoint for dry runs",
        );
    }
    let Some(key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|key| {
            (16..=128).contains(&key.len())
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    else {
        return api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_input",
            "Idempotency-Key must contain 16–128 letters, digits or hyphens",
        );
    };
    match execute(&state, &account, &request, key) {
        Ok(response) => response,
        Err(error) => {
            eprintln!("repository creation failed: {error:#}");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "creation_failed",
                "repository creation did not complete; inspect the server log before retrying",
            )
        }
    }
}

fn execute(
    state: &WebState,
    account: &AccountSummary,
    request: &CreateRepositoryRequest,
    key: &str,
) -> Result<Response> {
    let digest = hex::encode(Sha256::digest(serde_json::to_vec(request)?));
    let name = hex::encode(Sha256::digest(format!("{}\0{key}", account.login)));
    let directory = state
        .repo_manager
        .config()
        .storage_root
        .join(".jeryu-create-receipts");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)?;
    let path = directory.join(format!("{name}.json"));
    let mut reservation = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let previous: Receipt = match serde_json::from_slice(&fs::read(&path)?) {
                Ok(receipt) => receipt,
                Err(_) => {
                    return Ok(api_error(
                        StatusCode::CONFLICT,
                        "creation_incomplete",
                        "this creation is still in progress or requires recovery",
                    ));
                }
            };
            if previous.request_sha256 != digest {
                return Ok(api_error(
                    StatusCode::CONFLICT,
                    "idempotency_conflict",
                    "Idempotency-Key was used with a different request",
                ));
            }
            if let Some(id) = previous.repository_id {
                if let Ok(repo) = state.core.get_repository(&request.owner, &request.name)
                    && repo.id.to_string() == id
                {
                    return Ok(Json(repo_summary(state, &repo)).into_response());
                }
                return Ok(api_error(
                    StatusCode::CONFLICT,
                    "repository_changed",
                    "the original repository no longer exists",
                ));
            }
            return Ok(api_error(
                StatusCode::CONFLICT,
                "creation_incomplete",
                "this creation is still in progress or requires recovery",
            ));
        }
        Err(error) => return Err(error.into()),
    };
    serde_json::to_writer(
        &mut reservation,
        &Receipt {
            request_sha256: digest.clone(),
            repository_id: None,
        },
    )?;
    reservation.sync_all()?;
    fs::File::open(&directory)?.sync_all()?;

    if state
        .core
        .get_repository(&request.owner, &request.name)
        .is_ok()
    {
        return Ok(api_error(
            StatusCode::CONFLICT,
            "already_exists",
            "repository already exists",
        ));
    }
    let id = RepoId::new(&request.owner, &request.name)?;
    // Never adopt orphaned storage or initialize a ref in an existing repository.
    ensure!(
        !state.repo_manager.resolve(&id)?.path.exists(),
        "repository storage already exists"
    );
    let mut repo = state.core.create_repository(
        &request.owner,
        jeryu_core::CreateRepositoryRequest {
            name: request.name.clone(),
            private: request.visibility == RepositoryVisibility::Private,
            description: request.description.clone(),
            default_branch: request.default_branch.clone(),
        },
    )?;
    let bare = match state.repo_manager.open(&id) {
        Ok(bare) => bare,
        Err(_) => state.repo_manager.create_bare(&id)?,
    };
    state.repo_manager.install_pre_receive_hook(&bare)?;
    let branch = format!("refs/heads/{}", repo.default_branch);
    git(state, &bare.path, &["symbolic-ref", "HEAD", &branch], b"")?;
    if request.initialize_readme {
        let content = format!("# {}\n", request.name);
        let blob = git(
            state,
            &bare.path,
            &["hash-object", "-w", "--stdin"],
            content.as_bytes(),
        )?;
        let tree = git(
            state,
            &bare.path,
            &["mktree"],
            format!("100644 blob {blob}\tREADME.md\n").as_bytes(),
        )?;
        let commit = git(
            state,
            &bare.path,
            &["commit-tree", &tree],
            b"Initialize repository\n",
        )?;
        // Compare-and-swap only the absent initial ref; this cannot replace a pushed commit.
        git(
            state,
            &bare.path,
            &[
                "update-ref",
                &branch,
                &commit,
                "0000000000000000000000000000000000000000",
            ],
            b"",
        )?;
    }
    if request.family.is_some() {
        repo = state
            .core
            .set_repository_family(&repo.owner, &repo.name, request.family.clone())?;
    }
    let complete = directory.join(format!("{name}.complete"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&complete)?;
    serde_json::to_writer(
        &mut file,
        &Receipt {
            request_sha256: digest,
            repository_id: Some(repo.id.to_string()),
        },
    )?;
    file.sync_all()?;
    fs::rename(complete, path)?;
    fs::File::open(&directory)?.sync_all()?;
    Ok((StatusCode::CREATED, Json(repo_summary(state, &repo))).into_response())
}

fn git_command(state: &WebState) -> Command {
    let mut command = Command::new(&state.repo_manager.config().git_bin);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Jeryu")
        .env("GIT_AUTHOR_EMAIL", "jeryu@localhost")
        .env("GIT_COMMITTER_NAME", "Jeryu")
        .env("GIT_COMMITTER_EMAIL", "jeryu@localhost");
    command
}

fn git(state: &WebState, path: &Path, args: &[&str], input: &[u8]) -> Result<String> {
    let mut child = git_command(state)
        .current_dir(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().context("Git stdin")?.write_all(input)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Git {} failed: {}",
        args[0],
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
