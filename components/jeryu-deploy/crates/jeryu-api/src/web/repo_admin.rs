//! Destructive repository administration: `DELETE /api/v1/repos/:id`.
//!
//! Deletion is two-tier and audited. Tier one removes the repository from the
//! forge registry (one full-state persist — see `ForgeCore::delete_repository`);
//! tier two, only when explicitly requested, removes the managed bare git
//! directory after a battery of path-safety checks. The registry goes FIRST:
//! a crash between the tiers leaves a harmless orphaned bare directory, never
//! a registry row pointing at deleted storage.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response as AxumResponse};
use jeryu_core::{ForgeError, Repository};
use jeryu_gitd::RepoId;
use jeryu_gitd::import::{GitDirKind, classify_git_dir};
use jeryu_readmodel::contracts::{DeleteRepositoryReceipt, DeleteRepositoryRequest, DeletedCount};
use jeryu_runnerd::{WorkcellLease, WorkcellState};
use serde_json::json;

use super::repositories::{ApiErrorHint, api_error_with_hint, find_repo, repo_id};
use super::{WebState, api_error};

/// `DELETE /api/v1/repos/:id` — two-tier, audited repository deletion.
pub(super) async fn repo_delete(
    State(state): State<Arc<WebState>>,
    AxumPath(id): AxumPath<String>,
    body: Bytes,
) -> AxumResponse {
    let Some(repo) = find_repo(&state, &id) else {
        return delete_not_found();
    };
    let request: DeleteRepositoryRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return delete_invalid(&format!("body is not valid JSON: {error}"));
        }
    };
    // Byte-exact confirmation; wildcards and normalization are never accepted.
    if request.confirm_full_name != repo.full_name {
        return delete_invalid(&format!(
            "confirm_full_name must byte-match {:?}",
            repo.full_name
        ));
    }
    if let Some(reason) = live_work_blocker(&state, &repo) {
        return api_error_with_hint(
            StatusCode::CONFLICT,
            "conflict",
            "repository has live work",
            ApiErrorHint {
                purpose: "delete a repository",
                reason: "conflict",
                common_fixes: &[
                    "wait for live agent runs on this repository to finish",
                    "release workcells holding this repository before retrying",
                ],
                docs_url: "docs/errors.md#conflict",
                repair_hint: &format!("resolve the live work and retry ({reason})"),
            },
        );
    }

    let audit_id = match state.core.append_audit(
        "repository.delete",
        &repo.full_name,
        "requested",
        json!({ "delete_storage": request.delete_storage }),
    ) {
        Ok(audit_id) => audit_id,
        Err(error) => return delete_storage_failed("audit append", &error.to_string()),
    };

    // Tier one: registry removal (committed by the full-state persist).
    let deletion = match state.core.delete_repository(&repo.owner, &repo.name) {
        Ok(deletion) => deletion,
        Err(ForgeError::NotFound(_)) => return delete_not_found(),
        Err(error) => {
            let _ = state.core.append_audit(
                "repository.delete",
                &repo.full_name,
                "failed",
                json!({ "stage": "registry", "error": error.to_string() }),
            );
            return delete_storage_failed("registry delete", &error.to_string());
        }
    };
    let deleted_counts: Vec<DeletedCount> = deletion
        .removed_counts()
        .into_iter()
        .map(|(collection, removed)| DeletedCount {
            collection: collection.to_string(),
            removed,
        })
        .collect();

    // Tier two: managed storage, only on request and only after every safety
    // check in `delete_repo_storage` passes.
    let (storage_deleted, storage_path) = if request.delete_storage {
        // The doomed repo is already out of the registry, so every remaining
        // repository is an "other" for the alias check.
        let others: Vec<(String, String)> = state
            .core
            .list_repositories(None)
            .into_iter()
            .map(|other| (other.owner, other.name))
            .collect();
        match delete_repo_storage(
            &state.repo_manager.config().storage_root,
            &repo.owner,
            &repo.name,
            &others,
        ) {
            Ok(StorageDeletion::Deleted(path)) => (true, Some(path.display().to_string())),
            Ok(StorageDeletion::Missing) => (false, None),
            Err(reason) => {
                let _ = state.core.append_audit(
                    "repository.delete",
                    &repo.full_name,
                    "failed",
                    json!({
                        "stage": "storage",
                        "registry_deleted": true,
                        "deleted_counts": deleted_counts,
                        "reason": reason,
                    }),
                );
                return delete_invalid(&reason);
            }
        }
    } else {
        (false, None)
    };

    let receipt = DeleteRepositoryReceipt {
        repo: repo_id(&repo),
        registry_deleted: true,
        deleted_counts,
        storage_deleted,
        storage_path,
        audit_id,
    };
    if let Err(error) = state.core.append_audit(
        "repository.delete",
        &repo.full_name,
        "completed",
        json!({
            "registry_deleted": receipt.registry_deleted,
            "deleted_counts": receipt.deleted_counts,
            "storage_deleted": receipt.storage_deleted,
            "storage_path": receipt.storage_path,
        }),
    ) {
        // The deletion itself succeeded; surface the partial receipt so the
        // operator knows what was removed despite the missing audit row.
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "code": "storage_failed",
                "message": "repository was deleted but the completion audit entry could not be written",
                "detail": { "error": error.to_string(), "partial_receipt": receipt },
            })),
        )
            .into_response();
    }
    Json(receipt).into_response()
}

/// A human-readable reason when live work is bound to `repo`: running
/// repo-scoped agent runs, or workcells in a live state whose claimed repo
/// roots resolve to this repository.
fn live_work_blocker(state: &WebState, repo: &Repository) -> Option<String> {
    let running: Vec<String> = state
        .agent_runs
        .rows_for_repo(&repo.full_name)
        .into_iter()
        .filter(|row| row.status == "running")
        .map(|row| row.run_id)
        .collect();
    if !running.is_empty() {
        return Some(format!(
            "{} live agent run(s) on {}: {}",
            running.len(),
            repo.full_name,
            running.join(", ")
        ));
    }
    let expected_bare = state
        .repo_manager
        .config()
        .storage_root
        .join(&repo.owner)
        .join(format!("{}.git", repo.name));
    let live_cells: Vec<String> = state
        .workcells
        .lock()
        .expect("workcell manager mutex")
        .workcells()
        .into_iter()
        .filter(|cell| workcell_is_live(cell) && workcell_binds_repo(cell, repo, &expected_bare))
        .map(|cell| cell.workcell_id)
        .collect();
    if !live_cells.is_empty() {
        return Some(format!(
            "{} live workcell(s) holding {}: {}",
            live_cells.len(),
            repo.full_name,
            live_cells.join(", ")
        ));
    }
    None
}

/// A workcell counts as live work in every state but the warm-pool and
/// terminal ones.
fn workcell_is_live(cell: &WorkcellLease) -> bool {
    matches!(
        cell.state,
        WorkcellState::Claimed
            | WorkcellState::Held
            | WorkcellState::Repairing
            | WorkcellState::Blocked
    )
}

/// Does any of the workcell's claimed repo roots resolve to this repository?
/// Roots are host paths, so match the expected bare path exactly or an
/// `owner/name(.git)` path suffix.
fn workcell_binds_repo(cell: &WorkcellLease, repo: &Repository, expected_bare: &Path) -> bool {
    let plain_suffix = Path::new(&repo.owner).join(&repo.name);
    let bare_suffix = Path::new(&repo.owner).join(format!("{}.git", repo.name));
    cell.repo_roots.iter().any(|root| {
        root == expected_bare || root.ends_with(&plain_suffix) || root.ends_with(&bare_suffix)
    })
}

/// Outcome of a passed-all-checks storage deletion attempt.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum StorageDeletion {
    /// The bare directory at this canonical path was removed.
    Deleted(PathBuf),
    /// No managed directory exists for the repo: a registry-only outcome,
    /// not an error.
    Missing,
}

/// Remove the managed bare directory for `owner/name` under `storage_root`.
///
/// The check order is load-bearing:
///   a. the target path is computed ONLY from the registry identity
///      (`storage_root/owner/name.git`), never from user input;
///   b. every component from the storage root down is refused if it is a
///      symlink (a missing component is the benign `Missing` outcome);
///   c. the canonical target must sit under the canonical root at exactly
///      depth 2 (`owner/` + `name.git`);
///   d. no OTHER registered repository's expected path may canonicalize to
///      the same directory;
///   e. the target must look like a bare git repository;
///   f. only then is the tree removed.
pub(super) fn delete_repo_storage(
    storage_root: &Path,
    owner: &str,
    name: &str,
    other_repos: &[(String, String)],
) -> Result<StorageDeletion, String> {
    // (a) identity-derived path only.
    let id = RepoId::new(owner, name)
        .map_err(|error| format!("refusing storage delete: invalid repository id: {error}"))?;

    // (b) no symlinked components between the root and the target.
    let mut walk = storage_root.to_path_buf();
    for component in [id.owner.clone(), id.bare_name()] {
        walk.push(&component);
        match std::fs::symlink_metadata(&walk) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!(
                    "refusing storage delete: {} is a symlink",
                    walk.display()
                ));
            }
            Ok(_) => {}
            // A missing component means there is nothing managed to delete.
            Err(_) => return Ok(StorageDeletion::Missing),
        }
    }

    // (c) containment + exact depth under the canonical root.
    let root = storage_root.canonicalize().map_err(|error| {
        format!("refusing storage delete: storage root does not canonicalize: {error}")
    })?;
    let canonical = walk.canonicalize().map_err(|error| {
        format!("refusing storage delete: target does not canonicalize: {error}")
    })?;
    let relative = canonical.strip_prefix(&root).map_err(|_| {
        format!(
            "refusing storage delete: {} escapes the storage root {}",
            canonical.display(),
            root.display()
        )
    })?;
    if relative.components().count() != 2 {
        return Err(format!(
            "refusing storage delete: {} is not exactly <owner>/<name>.git under the storage root",
            canonical.display()
        ));
    }

    // (d) no other registered repository may alias the same directory.
    for (other_owner, other_name) in other_repos {
        if other_owner == owner && other_name == name {
            continue;
        }
        let Ok(other_id) = RepoId::new(other_owner, other_name) else {
            continue;
        };
        let other_path = storage_root
            .join(&other_id.owner)
            .join(other_id.bare_name());
        if let Ok(other_canonical) = other_path.canonicalize()
            && other_canonical == canonical
        {
            return Err(format!(
                "refusing storage delete: {} is also the storage of {}/{}",
                canonical.display(),
                other_owner,
                other_name
            ));
        }
    }

    // (e) the target must be shaped like a bare git repository.
    if classify_git_dir(&canonical) != Some(GitDirKind::Bare) {
        return Err(format!(
            "refusing storage delete: {} is not a bare git repository",
            canonical.display()
        ));
    }

    // (f) all checks passed.
    std::fs::remove_dir_all(&canonical)
        .map_err(|error| format!("storage delete failed: {error}"))?;
    Ok(StorageDeletion::Deleted(canonical))
}

fn delete_not_found() -> AxumResponse {
    api_error_with_hint(
        StatusCode::NOT_FOUND,
        "not_found",
        "repository not found",
        ApiErrorHint {
            purpose: "delete a repository",
            reason: "not_found",
            common_fixes: &[
                "verify the repository id or owner/name pair",
                "list /api/v1/repos to see the registered repositories",
            ],
            docs_url: "docs/errors.md#not-found",
            repair_hint: "rerun cargo test -p jeryu-api --features web --jobs 40",
        },
    )
}

fn delete_invalid(reason: &str) -> AxumResponse {
    api_error_with_hint(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_input",
        "repository delete request failed validation",
        ApiErrorHint {
            purpose: "delete a repository",
            reason: "invalid_input",
            common_fixes: &[
                "send a JSON body with confirm_full_name set to the exact owner/name",
                "set delete_storage true only for managed bare repositories",
            ],
            docs_url: "docs/errors.md#invalid-input",
            repair_hint: &format!("fix the DELETE body and retry ({reason})"),
        },
    )
}

fn delete_storage_failed(stage: &str, error: &str) -> AxumResponse {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "storage_failed",
        &format!("repository delete failed at {stage}: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Lay down a directory shaped like a bare git repository.
    fn write_bare(path: &Path) {
        std::fs::create_dir_all(path.join("objects")).expect("objects dir");
        std::fs::create_dir_all(path.join("refs")).expect("refs dir");
        std::fs::write(path.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD file");
    }

    #[test]
    fn storage_delete_removes_a_managed_bare_dir() {
        let root = tempdir().expect("storage root");
        let bare = root.path().join("alice").join("jeryu.git");
        write_bare(&bare);

        let outcome = delete_repo_storage(root.path(), "alice", "jeryu", &[]).expect("deleted");
        assert!(matches!(outcome, StorageDeletion::Deleted(_)));
        assert!(!bare.exists(), "the bare dir must be gone");
    }

    #[test]
    fn storage_delete_missing_dir_is_a_registry_only_outcome() {
        let root = tempdir().expect("storage root");
        assert_eq!(
            delete_repo_storage(root.path(), "alice", "jeryu", &[]),
            Ok(StorageDeletion::Missing)
        );
        // Owner dir present but the bare dir absent is Missing too.
        std::fs::create_dir_all(root.path().join("alice")).expect("owner dir");
        assert_eq!(
            delete_repo_storage(root.path(), "alice", "jeryu", &[]),
            Ok(StorageDeletion::Missing)
        );
    }

    #[cfg(unix)]
    #[test]
    fn storage_delete_refuses_symlinked_components_and_keeps_target() {
        let temp = tempdir().expect("base");
        let root = temp.path().join("storage");
        let outside = temp.path().join("outside").join("victim.git");
        write_bare(&outside);

        // Symlinked name.git component.
        std::fs::create_dir_all(root.join("alice")).expect("owner dir");
        std::os::unix::fs::symlink(&outside, root.join("alice").join("jeryu.git"))
            .expect("symlink bare");
        let reason =
            delete_repo_storage(&root, "alice", "jeryu", &[]).expect_err("must refuse symlink");
        assert!(reason.contains("symlink"), "{reason}");
        assert!(
            outside.join("HEAD").is_file(),
            "the symlink target must be untouched"
        );

        // Symlinked owner component.
        let owner_target = temp.path().join("elsewhere");
        write_bare(&owner_target.join("jeryu.git"));
        std::os::unix::fs::symlink(&owner_target, root.join("bob")).expect("symlink owner");
        let reason =
            delete_repo_storage(&root, "bob", "jeryu", &[]).expect_err("must refuse owner symlink");
        assert!(reason.contains("symlink"), "{reason}");
        assert!(owner_target.join("jeryu.git").join("HEAD").is_file());
    }

    /// Another registered repository whose expected path canonicalizes to the
    /// same directory (here through a symlinked owner dir) blocks deletion.
    #[cfg(unix)]
    #[test]
    fn storage_delete_refuses_aliased_registrations() {
        let temp = tempdir().expect("base");
        let root = temp.path().join("storage");
        let bare = root.join("alice").join("jeryu.git");
        write_bare(&bare);
        // bob/ is a symlink to alice/, so bob/jeryu.git aliases alice/jeryu.git.
        std::os::unix::fs::symlink(root.join("alice"), root.join("bob")).expect("alias owner");

        let others = vec![("bob".to_string(), "jeryu".to_string())];
        let reason = delete_repo_storage(&root, "alice", "jeryu", &others)
            .expect_err("must refuse the alias");
        assert!(reason.contains("also the storage of"), "{reason}");
        assert!(bare.join("HEAD").is_file(), "nothing may be deleted");
    }

    #[test]
    fn storage_delete_refuses_non_bare_directories() {
        let root = tempdir().expect("storage root");
        let not_bare = root.path().join("alice").join("jeryu.git");
        std::fs::create_dir_all(&not_bare).expect("plain dir");

        let reason = delete_repo_storage(root.path(), "alice", "jeryu", &[])
            .expect_err("must refuse a non-bare dir");
        assert!(reason.contains("not a bare git repository"), "{reason}");
        assert!(not_bare.exists());
    }
}
