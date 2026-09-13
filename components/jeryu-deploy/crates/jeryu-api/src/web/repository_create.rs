//! Authenticated repository creation for the browser's preview/execute contract.

#[cfg(test)]
use std::fs;
use std::io::Write;
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
use uuid::Uuid;

use super::{WebState, api_error, repositories::repo_summary};
use crate::git_materializer::{GitMaterializer, storage::Directory};

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
#[serde(deny_unknown_fields)]
struct Receipt {
    request_sha256: String,
    repository_id: Option<String>,
    #[serde(default)]
    creation_id: Option<Uuid>,
    #[serde(default)]
    initial_commit: Option<String>,
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
                "repository creation is pending; retry with the same settings; contact an administrator if it continues to fail",
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
    let directory_path = state
        .repo_manager
        .config()
        .storage_root
        .join(".jeryu-create-receipts");
    let directory = Directory::open(&directory_path)?;
    let _lock = directory.lock(&format!("{name}.lock"))?;
    let filename = format!("{name}.json");
    let mut receipt = if directory.exists(&filename)? {
        let previous: Receipt = directory.read(&filename)?;
        if previous.request_sha256 != digest {
            return Ok(api_error(
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "Idempotency-Key was used with a different request",
            ));
        }
        ensure!(
            previous
                .initial_commit
                .as_ref()
                .is_none_or(|commit| request.initialize_readme
                    && commit.len() == 40
                    && commit.bytes().all(|byte| byte.is_ascii_hexdigit())),
            "invalid creation commit receipt"
        );
        if let Some(id) = &previous.repository_id {
            if let Ok(repo) = state.core.get_repository(&request.owner, &request.name)
                && repo.id.to_string() == *id
            {
                if let Some(identity) = previous.creation_id {
                    ensure!(identity == repo.id, "creation receipt identity mismatch");
                    ensure!(
                        !request.initialize_readme || previous.initial_commit.is_some(),
                        "completed creation lacks its planned commit"
                    );
                    ensure!(
                        state
                            .core
                            .repository_creations()
                            .iter()
                            .any(|entry| entry.repository_id == identity && entry.materialized),
                        "Core creation is still pending"
                    );
                    GitMaterializer::new(state.repo_manager.clone()).verify_published(&repo)?;
                }
                return Ok(Json(repo_summary(state, &repo)).into_response());
            }
            return Ok(api_error(
                StatusCode::CONFLICT,
                "repository_changed",
                "the original repository no longer exists",
            ));
        }
        if previous.creation_id.is_none() {
            return Ok(api_error(
                StatusCode::CONFLICT,
                "creation_incomplete",
                "legacy interrupted creation has no durable identity; retain it for maintainer recovery",
            ));
        }
        previous
    } else {
        let receipt = Receipt {
            request_sha256: digest,
            repository_id: None,
            creation_id: Some(Uuid::new_v4()),
            initial_commit: None,
        };
        directory.write(&filename, &receipt, false)?;
        receipt
    };
    let creation_id = receipt.creation_id.context("missing creation identity")?;
    ensure!(!creation_id.is_nil(), "invalid creation identity");
    let id = RepoId::new(&request.owner, &request.name)?;
    if let Ok(existing) = state.core.get_repository(&request.owner, &request.name) {
        if existing.id != creation_id {
            return Ok(api_error(
                StatusCode::CONFLICT,
                "repository_changed",
                "repository belongs to another creation",
            ));
        }
    } else {
        // Core did not start this request. It cannot adopt an orphaned Git path.
        match std::fs::symlink_metadata(state.repo_manager.resolve(&id)?.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => anyhow::bail!("repository storage already exists"),
        }
    }
    let mut repo = state.core.create_repository_with_id(
        creation_id,
        &request.owner,
        jeryu_core::CreateRepositoryRequest {
            name: request.name.clone(),
            private: request.visibility == RepositoryVisibility::Private,
            description: request.description.clone(),
            default_branch: request.default_branch.clone(),
        },
    )?;
    // Embedded/test callers may use metadata-only Core. The same identity-bound
    // materializer is safe to replay after production Core already completed it.
    GitMaterializer::new(state.repo_manager.clone()).resume(&repo)?;
    let bare = state.repo_manager.open(&id)?;
    let branch = format!("refs/heads/{}", repo.default_branch);
    if request.initialize_readme {
        if receipt.initial_commit.is_none() {
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
            // Persist the exact object before touching a visible ref. A crash
            // after update-ref resumes this commit instead of creating another.
            receipt.initial_commit = Some(commit);
            crate::git_materializer::sync_repository(&bare.path)?;
            directory.write(&filename, &receipt, true)?;
        }
        let commit = receipt
            .initial_commit
            .as_deref()
            .context("missing initial commit")?;
        ensure!(
            commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid planned initial commit"
        );
        // Compare-and-swap only the absent initial ref. On replay, require that
        // this exact initialization survived, possibly beneath later pushes.
        let observed = git_command(state)
            .current_dir(&bare.path)
            .args(["show-ref", "--verify", "--quiet", &branch])
            .output()?;
        if observed.status.success() {
            git(
                state,
                &bare.path,
                &["merge-base", "--is-ancestor", commit, &branch],
                b"",
            )?;
        } else {
            ensure!(
                observed.status.code() == Some(1),
                "cannot read the initial repository ref"
            );
            git(
                state,
                &bare.path,
                &[
                    "update-ref",
                    &branch,
                    commit,
                    "0000000000000000000000000000000000000000",
                ],
                b"",
            )?;
        }
        // update-ref is logically atomic; fsync objects and refs before the
        // browser receipt can acknowledge durable completion.
        crate::git_materializer::sync_repository(&bare.path)?;
    }
    if request.family.is_some() {
        repo = state.core.set_repository_family_with_id(
            &repo.owner,
            &repo.name,
            repo.id,
            request.family.clone(),
        )?;
    }
    receipt.repository_id = Some(repo.id.to_string());
    directory.write(&filename, &receipt, true)?;
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
