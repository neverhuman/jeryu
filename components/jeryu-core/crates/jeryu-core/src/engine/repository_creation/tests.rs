use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::*;
use crate::RepoMaterializer;

#[derive(Debug, Default)]
struct Materializer {
    fail: AtomicBool,
    calls: AtomicUsize,
}

impl RepoMaterializer for Materializer {
    fn materialize(&self, _owner: &str, _name: &str, _branch: &str) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(ForgeError::Storage("interrupted Git initialization".into()));
        }
        Ok(())
    }
}

fn request() -> CreateRepositoryRequest {
    CreateRepositoryRequest {
        name: "recovery".into(),
        private: true,
        description: Some("durable request".into()),
        default_branch: Some("trunk".into()),
    }
}

#[test]
fn creation_family_completion_cannot_change_a_replacement_repository() {
    let core = ForgeCore::new();
    let original = core.create_repository("alice", request()).unwrap();
    core.delete_repository("alice", "recovery").unwrap();
    let replacement = core.create_repository("alice", request()).unwrap();
    assert!(
        core.set_repository_family_with_id(
            "alice",
            "recovery",
            original.id,
            Some("previous".into())
        )
        .is_err()
    );
    assert_eq!(
        core.get_repository("alice", "recovery").unwrap().family,
        None
    );
    assert_eq!(
        core.set_repository_family_with_id(
            "alice",
            "recovery",
            replacement.id,
            Some("current".into())
        )
        .unwrap()
        .family
        .as_deref(),
        Some("current")
    );
}

#[test]
fn failed_materialization_resumes_exact_identity_after_reopen_and_unrelated_write() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("forge.sqlite");
    let materializer = Arc::new(Materializer::default());
    materializer.fail.store(true, Ordering::SeqCst);
    let id = Uuid::new_v4();
    let core = ForgeCore::open_sqlite(&database)
        .unwrap()
        .with_repo_materializer(materializer.clone());
    assert!(
        core.create_repository_with_id(id, "alice", request())
            .is_err()
    );
    assert_eq!(core.get_repository("alice", "recovery").unwrap().id, id);
    assert!(!core.repository_creations()[0].materialized);
    core.set_repository_family("alice", "recovery", Some("retained".into()))
        .unwrap();
    drop(core);

    materializer.fail.store(false, Ordering::SeqCst);
    let core = ForgeCore::open_sqlite(&database)
        .unwrap()
        .with_repo_materializer(materializer.clone());
    assert!(!core.repository_creations()[0].materialized);
    let completed = core
        .create_repository_with_id(id, "alice", request())
        .unwrap();
    assert_eq!(completed.id, id);
    assert_eq!(completed.family.as_deref(), Some("retained"));
    assert!(core.repository_creations()[0].materialized);
    assert_eq!(materializer.calls.load(Ordering::SeqCst), 2);
    drop(core);
    let core = ForgeCore::open_sqlite(&database)
        .unwrap()
        .with_repo_materializer(materializer.clone());
    assert_eq!(
        core.create_repository_with_id(id, "alice", request())
            .unwrap(),
        completed
    );
    assert_eq!(materializer.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn retry_rejects_changed_request_owner_and_another_creation_id() {
    let core = ForgeCore::new();
    let id = Uuid::new_v4();
    let original = core
        .create_repository_with_id(id, "alice", request())
        .unwrap();
    for changed in [
        CreateRepositoryRequest {
            private: false,
            ..request()
        },
        CreateRepositoryRequest {
            name: "another".into(),
            ..request()
        },
        CreateRepositoryRequest {
            description: None,
            ..request()
        },
        CreateRepositoryRequest {
            default_branch: None,
            ..request()
        },
    ] {
        assert!(
            core.create_repository_with_id(id, "alice", changed)
                .is_err()
        );
    }
    assert!(
        core.create_repository_with_id(id, "bob", request())
            .is_err()
    );
    assert!(
        core.create_repository_with_id(Uuid::new_v4(), "alice", request())
            .is_err()
    );
    assert!(
        core.create_repository_with_id(Uuid::nil(), "alice", request())
            .is_err()
    );
    assert_eq!(core.list_repositories(None), vec![original]);
    assert_eq!(core.repository_creations().len(), 1);
}

#[test]
fn retained_creation_cannot_resurrect_deleted_or_recreated_repository() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let id = Uuid::new_v4();
    core.create_repository_with_id(id, "alice", request())
        .unwrap();
    core.delete_repository("alice", "recovery").unwrap();
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert!(
        core.create_repository_with_id(id, "alice", request())
            .is_err()
    );
    assert!(core.list_repositories(None).is_empty());
    let replacement = core.create_repository("alice", request()).unwrap();
    assert_ne!(replacement.id, id);
    assert!(
        core.create_repository_with_id(id, "alice", request())
            .is_err()
    );
    assert_eq!(core.list_repositories(None), vec![replacement]);
    assert_eq!(core.repository_creations().len(), 2);
}

#[test]
fn completion_write_failure_is_an_error_and_retains_pending_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_completion BEFORE INSERT ON repository_creation_journal WHEN json_extract(NEW.receipt_json, '$.materialized') = 1 BEGIN SELECT RAISE(ABORT, 'interrupted completion write'); END;").unwrap();
    let id = Uuid::new_v4();
    assert!(
        core.create_repository_with_id(id, "alice", request())
            .is_err()
    );
    assert!(!core.repository_creations()[0].materialized);
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert!(!core.repository_creations()[0].materialized);
    connection
        .execute_batch("DROP TRIGGER reject_completion")
        .unwrap();
    assert_eq!(
        core.create_repository_with_id(id, "alice", request())
            .unwrap()
            .id,
        id
    );
    assert!(core.repository_creations()[0].materialized);
}

#[test]
fn migration_preserves_legacy_repositories_without_inventing_retry_authority() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let original = core.create_repository("alice", request()).unwrap();
    drop(core);
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute_batch("DROP TABLE repository_creation_journal")
        .unwrap();
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(core.get_repository("alice", "recovery").unwrap(), original);
    assert!(core.repository_creations().is_empty());
    assert!(
        core.create_repository_with_id(original.id, "alice", request())
            .is_err()
    );
    core.set_repository_family("alice", "recovery", Some("unchanged".into()))
        .unwrap();
    drop(core);
    assert!(
        ForgeCore::open_sqlite(&database)
            .unwrap()
            .repository_creations()
            .is_empty()
    );
}

#[test]
fn stored_receipt_identity_mismatch_refuses_open() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    core.create_repository("alice", request()).unwrap();
    drop(core);
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE repository_creation_journal SET repository_id = ?1",
            [Uuid::new_v4().to_string()],
        )
        .unwrap();
    assert!(ForgeCore::open_sqlite(&database).is_err());
}
