use super::*;
use jeryu_core::{CreateRepositoryRequest, ForgeCore};
use jeryu_gitd::GitdConfig;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

fn directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

fn request() -> CreateRepositoryRequest {
    CreateRepositoryRequest {
        name: "first".into(),
        private: true,
        description: None,
        default_branch: Some("trunk".into()),
    }
}

#[test]
fn creation_publishes_complete_git_and_retry_preserves_changed_head() {
    let directory = directory();
    let manager = Arc::new(RepoManager::new(GitdConfig::new(
        directory.path().join("git"),
    )));
    let materializer = Arc::new(GitMaterializer::new(manager.clone()));
    let core = ForgeCore::new().with_repo_materializer(materializer.clone());
    let id = uuid::Uuid::new_v4();
    let repo = core
        .create_repository_with_id(id, "alice", request())
        .unwrap();
    let bare = manager.open_parts("alice", "first").unwrap();
    assert_eq!(
        fs::read_to_string(bare.path.join("HEAD")).unwrap(),
        "ref: refs/heads/trunk\n"
    );
    assert!(bare.path.join("hooks/pre-receive").is_file());
    assert_eq!(
        fs::read_to_string(bare.path.join("jeryu/repo-id")).unwrap(),
        "alice/first"
    );
    materializer
        .git(&bare.path, &["symbolic-ref", "HEAD", "refs/heads/changed"])
        .unwrap();
    materializer.resume(&repo).unwrap();
    assert_eq!(
        fs::read_to_string(bare.path.join("HEAD")).unwrap(),
        "ref: refs/heads/changed\n"
    );
    assert!(core.repository_creations()[0].materialized);
    assert_eq!(
        core.create_repository_with_id(id, "alice", request())
            .unwrap()
            .id,
        repo.id
    );
}

#[test]
fn failed_git_initialization_is_pending_and_replays_after_database_restart() {
    let directory = directory();
    let launcher = directory.path().join("git-launcher");
    fs::write(&launcher, "#!/bin/sh\nif [ \"$1\" = init ]; then mkdir -p objects; exit 73; fi\nexec /usr/bin/git \"$@\"\n").unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = GitdConfig::new(directory.path().join("git"));
    config.git_bin = launcher.to_string_lossy().into_owned();
    let manager = Arc::new(RepoManager::new(config));
    let materializer = Arc::new(GitMaterializer::new(manager.clone()));
    let database = directory.path().join("core.sqlite");
    let core = ForgeCore::open_sqlite(&database)
        .unwrap()
        .with_repo_materializer(materializer.clone());
    let id = uuid::Uuid::new_v4();
    assert!(
        core.create_repository_with_id(id, "alice", request())
            .is_err()
    );
    assert!(!core.repository_creations()[0].materialized);
    assert!(
        !manager
            .resolve_parts("alice", "first")
            .unwrap()
            .path
            .exists()
    );
    drop(core);
    fs::write(&launcher, "#!/bin/sh\nexec /usr/bin/git \"$@\"\n").unwrap();
    let core = ForgeCore::open_sqlite(&database)
        .unwrap()
        .with_repo_materializer(materializer);
    assert_eq!(
        core.create_repository_with_id(id, "alice", request())
            .unwrap()
            .id,
        id
    );
    assert!(core.repository_creations()[0].materialized);
    assert!(
        manager
            .open_parts("alice", "first")
            .unwrap()
            .path
            .join("objects")
            .is_dir()
    );
}

#[test]
fn orphan_collision_symlink_and_creation_identity_mismatch_are_preserved() {
    for kind in ["orphan", "symlink", "identity"] {
        let directory = directory();
        let manager = Arc::new(RepoManager::new(GitdConfig::new(
            directory.path().join("git"),
        )));
        let materializer = Arc::new(GitMaterializer::new(manager.clone()));
        let core = ForgeCore::new().with_repo_materializer(materializer.clone());
        let id = uuid::Uuid::new_v4();
        let orphan = manager
            .create_bare(&RepoId::new("alice", "first").unwrap())
            .unwrap();
        let original = fs::read(orphan.path.join("HEAD")).unwrap();
        if kind == "symlink" {
            let moved = directory.path().join("original");
            fs::rename(&orphan.path, &moved).unwrap();
            symlink(moved, &orphan.path).unwrap();
        }
        if kind == "identity" {
            Directory::open(&orphan.path)
                .unwrap()
                .write(
                    "jeryu-creation.json",
                    &Identity {
                        id: uuid::Uuid::new_v4(),
                        owner: "alice".into(),
                        name: "first".into(),
                        branch: "trunk".into(),
                    },
                    false,
                )
                .unwrap();
        }
        assert!(
            core.create_repository_with_id(id, "alice", request())
                .is_err(),
            "{kind}"
        );
        assert!(
            core.create_repository_with_id(id, "alice", request())
                .is_err(),
            "{kind} retry"
        );
        assert_eq!(fs::read(orphan.path.join("HEAD")).unwrap(), original);
        assert!(!core.repository_creations()[0].materialized);
    }
}

#[test]
fn invalid_git_branch_fails_before_metadata_or_storage_mutation() {
    let directory = directory();
    let storage = directory.path().join("git");
    let core = ForgeCore::new().with_repo_materializer(Arc::new(GitMaterializer::new(Arc::new(
        RepoManager::new(GitdConfig::new(&storage)),
    ))));
    for branch in ["../outside", "trunk.lock", "-option", "bad\nbranch"] {
        let mut request = request();
        request.default_branch = Some(branch.into());
        assert!(core.create_repository("alice", request).is_err());
    }
    assert!(core.list_repositories(None).is_empty());
    assert!(core.repository_creations().is_empty());
    assert!(!storage.exists());
}

#[test]
fn receipt_lock_serializes_attempts_and_unsafe_files_cannot_supply_identity() {
    let directory = directory();
    let held = Directory::open(directory.path()).unwrap();
    let first = held.lock("creation.lock").unwrap();
    assert!(held.lock("creation.lock").is_err());
    drop(first);
    assert!(held.lock("creation.lock").is_ok());
    held.write("receipt.json", &serde_json::json!({"id": 1}), false)
        .unwrap();
    assert!(
        held.write("receipt.json", &serde_json::json!({"id": 2}), false)
            .is_err()
    );
    assert_eq!(
        held.read::<serde_json::Value>("receipt.json").unwrap()["id"],
        1
    );
    fs::hard_link(
        directory.path().join("receipt.json"),
        directory.path().join("alias.json"),
    )
    .unwrap();
    assert!(held.read::<serde_json::Value>("receipt.json").is_err());
    symlink(
        directory.path().join("receipt.json"),
        directory.path().join("linked.json"),
    )
    .unwrap();
    assert!(held.read::<serde_json::Value>("linked.json").is_err());
}
