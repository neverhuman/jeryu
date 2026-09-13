use super::*;
use axum::body::to_bytes;
use jeryu_core::{AccountStatus, ForgeCore};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;

fn directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

fn receipt_location(state: &WebState) -> (Directory, String) {
    let directory = Directory::open(
        &state
            .repo_manager
            .config()
            .storage_root
            .join(".jeryu-create-receipts"),
    )
    .unwrap();
    let name = hex::encode(Sha256::digest("alice\0repository-create-fixture"));
    (directory, format!("{name}.json"))
}

fn open_sqlite_after_last_writer(path: &Path) -> ForgeCore {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match ForgeCore::open_sqlite(path) {
            Ok(core) => return core,
            Err(jeryu_core::ForgeError::WriterUnavailable(message))
                if std::time::Instant::now() < deadline
                    && message.contains("resource lease refused")
                    && message.contains("would block") =>
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => panic!("reopen after last writer dropped: {error}"),
        }
    }
}

fn account() -> AccountSummary {
    AccountSummary {
        login: "alice".into(),
        display_name: "Alice".into(),
        role: UserRole::User,
        status: AccountStatus::Active,
        auth_epoch: 0,
        must_change_password: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn request() -> CreateRepositoryRequest {
    CreateRepositoryRequest {
        host: "jeryu".into(),
        owner: "alice".into(),
        name: "first".into(),
        description: Some("first repository".into()),
        visibility: RepositoryVisibility::Private,
        initialize_readme: true,
        gitignore_template: None,
        license_template: None,
        default_branch: Some("trunk".into()),
        topics: vec![],
        family: Some("example".into()),
        template: None,
        dry_run: false,
    }
}

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "idempotency-key",
        "repository-create-fixture".parse().unwrap(),
    );
    headers
}

async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap()
}

#[tokio::test]
async fn preview_is_read_only_and_create_replays_after_database_reopen() {
    let dir = directory();
    let database = dir.path().join("forge.sqlite");
    let storage = dir.path().join("git");
    let state = Arc::new(WebState::new_with_git_storage(
        ForgeCore::open_sqlite(&database).unwrap(),
        storage.clone(),
    ));
    let mut dry = request();
    dry.dry_run = true;
    let response = preview(State(state.clone()), Extension(account()), Json(dry)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body(response).await["initial_files"],
        serde_json::json!(["README.md"])
    );
    assert!(state.core.list_repositories(None).is_empty());
    assert!(
        !storage.exists(),
        "preview must not create storage or receipts"
    );

    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = body(response).await;
    let bare = state.repo_manager.open_parts("alice", "first").unwrap();
    assert_eq!(
        git(&state, &bare.path, &["symbolic-ref", "HEAD"], b"").unwrap(),
        "refs/heads/trunk"
    );
    assert_eq!(
        git(&state, &bare.path, &["show", "HEAD:README.md"], b"").unwrap(),
        "# first"
    );
    assert_eq!(created["family"], "example");
    drop(state);

    let reopened = Arc::new(WebState::new_with_git_storage(
        open_sqlite_after_last_writer(&database),
        storage,
    ));
    let response = create(
        State(reopened.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await["id"], created["id"]);
    assert_eq!(reopened.core.list_repositories(None).len(), 1);
    let mut changed = request();
    changed.description = Some("different request".into());
    let response = create(
        State(reopened),
        Extension(account()),
        headers(),
        Json(changed),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(body(response).await["code"], "idempotency_conflict");
}

#[tokio::test]
async fn creation_rejects_other_owners_invalid_paths_and_missing_keys_without_mutation() {
    let dir = directory();
    let storage = dir.path().join("git");
    let state = Arc::new(WebState::new_with_git_storage(
        ForgeCore::new(),
        storage.clone(),
    ));
    let mut other_owner = request();
    other_owner.owner = "bob".into();
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(other_owner),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    for name in ["../escape", "repo.git", "-option", "bad/name"] {
        let mut invalid = request();
        invalid.name = name.into();
        let response = create(
            State(state.clone()),
            Extension(account()),
            headers(),
            Json(invalid),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{name}"
        );
    }
    let response = create(
        State(state.clone()),
        Extension(account()),
        HeaderMap::new(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(state.core.list_repositories(None).is_empty());
    assert!(!storage.exists());
}

#[tokio::test]
async fn creation_never_adopts_or_changes_orphaned_git_storage() {
    let dir = directory();
    let state = Arc::new(WebState::new_with_git_storage(
        ForgeCore::new(),
        dir.path().join("git"),
    ));
    let bare = state
        .repo_manager
        .create_bare(&RepoId::new("alice", "first").unwrap())
        .unwrap();
    let original_head = fs::read(bare.path.join("HEAD")).unwrap();
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let retry = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(retry.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body(retry).await["code"], "creation_failed");
    assert!(state.core.list_repositories(None).is_empty());
    assert_eq!(fs::read(bare.path.join("HEAD")).unwrap(), original_head);
}

#[tokio::test]
async fn reserved_creation_resumes_its_uuid_and_legacy_pending_receipts_stay_closed() {
    for legacy in [false, true] {
        let dir = directory();
        let state = Arc::new(WebState::new_with_git_storage(
            ForgeCore::new(),
            dir.path().join("git"),
        ));
        let (directory, filename) = receipt_location(&state);
        let identity = Uuid::new_v4();
        directory
            .write(
                &filename,
                &Receipt {
                    request_sha256: hex::encode(Sha256::digest(
                        serde_json::to_vec(&request()).unwrap(),
                    )),
                    repository_id: None,
                    creation_id: (!legacy).then_some(identity),
                    initial_commit: None,
                },
                false,
            )
            .unwrap();
        let response = create(
            State(state.clone()),
            Extension(account()),
            headers(),
            Json(request()),
        )
        .await;
        if legacy {
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(body(response).await["code"], "creation_incomplete");
            assert!(state.core.list_repositories(None).is_empty());
        } else {
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(body(response).await["entity"]["id"], identity.to_string());
        }
    }
}

#[tokio::test]
async fn interrupted_completion_reopens_and_preserves_a_pushed_descendant() {
    let dir = directory();
    let database = dir.path().join("core.sqlite");
    let storage = dir.path().join("git");
    let state = Arc::new(WebState::new_with_git_storage(
        ForgeCore::open_sqlite(&database).unwrap(),
        storage.clone(),
    ));
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let repo = body(response).await;
    let (directory, filename) = receipt_location(&state);
    let mut receipt: Receipt = directory.read(&filename).unwrap();
    // Model the durable state immediately after ref publication, before the
    // final browser receipt was committed. Keep its exact planned commit.
    receipt.repository_id = None;
    directory.write(&filename, &receipt, true).unwrap();
    let bare = state.repo_manager.open_parts("alice", "first").unwrap();
    let tree = git(&state, &bare.path, &["rev-parse", "HEAD^{tree}"], b"").unwrap();
    let pushed = git(
        &state,
        &bare.path,
        &["commit-tree", &tree, "-p", "HEAD"],
        b"User push\n",
    )
    .unwrap();
    git(
        &state,
        &bare.path,
        &["update-ref", "refs/heads/trunk", &pushed],
        b"",
    )
    .unwrap();
    drop(state);
    let state = Arc::new(WebState::new_with_git_storage(
        open_sqlite_after_last_writer(&database),
        storage,
    ));
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(body(response).await["id"], repo["id"]);
    assert_eq!(
        git(&state, &bare.path, &["rev-parse", "HEAD"], b"").unwrap(),
        pushed
    );
    assert_eq!(
        directory.read::<Receipt>(&filename).unwrap().initial_commit,
        receipt.initial_commit
    );
    assert_eq!(state.core.list_repositories(None).len(), 1);
}

#[tokio::test]
async fn interrupted_initialization_never_replaces_an_unrelated_pushed_branch() {
    let dir = directory();
    let state = Arc::new(WebState::new_with_git_storage(
        ForgeCore::new(),
        dir.path().join("git"),
    ));
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let (directory, filename) = receipt_location(&state);
    let mut receipt: Receipt = directory.read(&filename).unwrap();
    receipt.repository_id = None;
    directory.write(&filename, &receipt, true).unwrap();
    let bare = state.repo_manager.open_parts("alice", "first").unwrap();
    let tree = git(&state, &bare.path, &["rev-parse", "HEAD^{tree}"], b"").unwrap();
    let unrelated = git(
        &state,
        &bare.path,
        &["commit-tree", &tree],
        b"Unrelated push\n",
    )
    .unwrap();
    git(
        &state,
        &bare.path,
        &["update-ref", "refs/heads/trunk", &unrelated],
        b"",
    )
    .unwrap();
    let response = create(
        State(state.clone()),
        Extension(account()),
        headers(),
        Json(request()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        git(&state, &bare.path, &["rev-parse", "HEAD"], b"").unwrap(),
        unrelated
    );
    assert!(
        directory
            .read::<Receipt>(&filename)
            .unwrap()
            .repository_id
            .is_none()
    );
}

#[tokio::test]
async fn completed_replay_refuses_missing_git_or_incomplete_commit_receipts() {
    for missing_git in [true, false] {
        let dir = directory();
        let state = Arc::new(WebState::new_with_git_storage(
            ForgeCore::new(),
            dir.path().join("git"),
        ));
        let response = create(
            State(state.clone()),
            Extension(account()),
            headers(),
            Json(request()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let bare = state.repo_manager.open_parts("alice", "first").unwrap();
        if missing_git {
            fs::rename(&bare.path, dir.path().join("retained-git")).unwrap();
        } else {
            let (directory, filename) = receipt_location(&state);
            let mut receipt: Receipt = directory.read(&filename).unwrap();
            receipt.initial_commit = None;
            directory.write(&filename, &receipt, true).unwrap();
        }
        let response = create(
            State(state.clone()),
            Extension(account()),
            headers(),
            Json(request()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        if missing_git {
            assert!(
                !bare.path.exists(),
                "replay must not recreate a removed completed repository"
            );
            assert!(dir.path().join("retained-git/HEAD").is_file());
        }
    }
}
