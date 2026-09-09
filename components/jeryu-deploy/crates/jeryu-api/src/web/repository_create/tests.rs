use super::*;
use axum::body::to_bytes;
use jeryu_core::{AccountStatus, ForgeCore};
use serde_json::Value;

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
    let dir = tempfile::tempdir().unwrap();
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
        ForgeCore::open_sqlite(database).unwrap(),
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
    let dir = tempfile::tempdir().unwrap();
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
    let dir = tempfile::tempdir().unwrap();
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
    assert_eq!(retry.status(), StatusCode::CONFLICT);
    assert_eq!(body(retry).await["code"], "creation_incomplete");
    assert!(state.core.list_repositories(None).is_empty());
    assert_eq!(fs::read(bare.path.join("HEAD")).unwrap(), original_head);
}
