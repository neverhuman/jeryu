//! Real foreign-key children and opaque columns survive State-owned persistence.

mod support;

use std::path::PathBuf;

use jeryu_core::*;
use rusqlite::{Connection, params};
use support::private_directory;
use tempfile::TempDir;
use uuid::Uuid;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PAYLOAD: &str = "durable execution evidence";

struct Parents {
    repository: Uuid,
    pull: Uuid,
    check: Uuid,
}

impl Parents {
    fn rows(&self) -> [(&'static str, &'static str, String); 5] {
        [
            ("repositories", "id", self.repository.to_string()),
            (
                "repository_creation_journal",
                "repository_id",
                self.repository.to_string(),
            ),
            ("users", "login", "owner".into()),
            ("pull_requests", "id", self.pull.to_string()),
            ("check_runs", "id", self.check.to_string()),
        ]
    }
}

fn fixture() -> (TempDir, PathBuf, ForgeCore, Parents) {
    let directory = private_directory();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    core.create_account("owner", "correct horse battery", UserRole::Admin)
        .unwrap();
    let repository = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "demo".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let pull = core
        .create_pull_request(
            "owner",
            "demo",
            "owner",
            CreatePullRequestRequest {
                title: "change".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(HEAD.into()),
                ..Default::default()
            },
        )
        .unwrap();
    let check = core
        .create_check_run(
            "owner",
            "demo",
            CreateCheckRunRequest {
                name: "demo/required".into(),
                head_sha: HEAD.into(),
                ..Default::default()
            },
        )
        .unwrap();
    core.append_audit(
        "fixture.seed",
        "owner/demo",
        "completed",
        serde_json::json!({}),
    )
    .unwrap();
    let parents = Parents {
        repository: repository.id,
        pull: pull.id,
        check: check.id,
    };
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE future_ci_receipts (
                 id TEXT PRIMARY KEY,
                 repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                 login TEXT NOT NULL REFERENCES users(login) ON DELETE CASCADE,
                 pull_id TEXT NOT NULL REFERENCES pull_requests(id) ON DELETE CASCADE,
                 check_id TEXT NOT NULL REFERENCES check_runs(id) ON DELETE CASCADE,
                 payload TEXT NOT NULL
             );
             CREATE TABLE future_outbox (
                 id TEXT PRIMARY KEY,
                 repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                 payload TEXT NOT NULL
             );
             CREATE TABLE future_creation_receipts (
                 repository_id TEXT PRIMARY KEY REFERENCES repository_creation_journal(repository_id) ON DELETE CASCADE,
                 payload TEXT NOT NULL
             );
             CREATE TABLE observed_parent_writes (
                 table_name TEXT NOT NULL,
                 parent_key TEXT NOT NULL,
                 event TEXT NOT NULL
             );
             ALTER TABLE repositories ADD COLUMN future_default TEXT NOT NULL DEFAULT 'unset';",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO future_ci_receipts VALUES ('receipt', ?1, 'owner', ?2, ?3, ?4)",
            params![
                parents.repository.to_string(),
                parents.pull.to_string(),
                parents.check.to_string(),
                PAYLOAD
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO future_outbox VALUES ('delivery', ?1, 'pending notification')",
            params![parents.repository.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO future_creation_receipts VALUES (?1, 'retain creation identity')",
            [parents.repository.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE repositories SET future_default = 'preserved binding' WHERE id = ?1",
            params![parents.repository.to_string()],
        )
        .unwrap();
    for (table, key_column, key) in parents.rows() {
        connection
            .execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN future_opaque BLOB;"
            ))
            .unwrap();
        connection
            .execute(
                &format!("UPDATE {table} SET future_opaque = ?1 WHERE {key_column} = ?2"),
                params![vec![0_u8, 17, 128, 255], key],
            )
            .unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TRIGGER observe_{table}_insert BEFORE INSERT ON {table}
                 BEGIN INSERT INTO observed_parent_writes VALUES ('{table}', NEW.{key_column}, 'insert'); END;
                 CREATE TRIGGER observe_{table}_update AFTER UPDATE ON {table}
                 BEGIN INSERT INTO observed_parent_writes VALUES ('{table}', NEW.{key_column}, 'update'); END;"
            ))
            .unwrap();
    }
    (directory, database, core, parents)
}

fn parent_rows(connection: &Connection, parents: &Parents) -> Vec<(String, i64, Vec<u8>)> {
    parents
        .rows()
        .into_iter()
        .map(|(table, key_column, key)| {
            let (rowid, opaque) = connection
                .query_row(
                    &format!("SELECT rowid, future_opaque FROM {table} WHERE {key_column} = ?1"),
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            (table.to_string(), rowid, opaque)
        })
        .collect()
}

fn assert_external_evidence(connection: &Connection) {
    let payload: String = connection
        .query_row(
            "SELECT payload FROM future_ci_receipts WHERE id = 'receipt'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, PAYLOAD);
    let outbox: String = connection
        .query_row(
            "SELECT payload FROM future_outbox WHERE id = 'delivery'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(outbox, "pending notification");
    assert_creation_receipt(connection);
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}

fn assert_creation_receipt(connection: &Connection) {
    let payload: String = connection
        .query_row("SELECT payload FROM future_creation_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(payload, "retain creation identity");
}

fn transfer(repository_id: Uuid) -> PrepareRepositoryTransfer {
    PrepareRepositoryTransfer {
        repository_id,
        expected_source_owner: "owner".into(),
        expected_source_name: "demo".into(),
        destination_owner: "destination".into(),
        request_fingerprint: "reviewed-transfer".into(),
        idempotency_key: "transfer".into(),
    }
}

#[test]
fn unrelated_saves_preserve_external_children_opaque_columns_and_parent_rowids() {
    let (_directory, database, core, parents) = fixture();
    let connection = Connection::open(&database).unwrap();
    let before = parent_rows(&connection, &parents);

    core.create_user(CreateUserRequest {
        login: "unrelated".into(),
        ..Default::default()
    })
    .unwrap();
    core.set_repository_readme("owner", "demo", "new readme".into())
        .unwrap();
    let new_repository = core
        .create_repository(
            "unrelated",
            CreateRepositoryRequest {
                name: "other".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_external_evidence(&connection);
    assert_eq!(parent_rows(&connection, &parents), before);
    for (table, _, key) in parents.rows() {
        let writes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM observed_parent_writes WHERE table_name = ?1 AND parent_key = ?2",
                params![table, key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            writes, 0,
            "unchanged parent {table} must not run write triggers"
        );
    }
    let default: String = connection
        .query_row(
            "SELECT future_default FROM repositories WHERE id = ?1",
            [new_repository.id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(default, "unset");
    assert_eq!(core.list_audit("owner/demo").unwrap().len(), 1);
    drop(connection);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    let connection = Connection::open(&database).unwrap();
    assert_external_evidence(&connection);
    assert_eq!(parent_rows(&connection, &parents), before);
    assert_eq!(reopened.list_audit("owner/demo").unwrap().len(), 1);
    assert_eq!(
        reopened
            .get_repository_readme("owner", "demo")
            .unwrap()
            .as_deref(),
        Some("new readme")
    );
}

#[test]
fn changed_owned_fields_and_transfer_preserve_parent_keys_and_future_columns() {
    let (_directory, database, core, parents) = fixture();
    let connection = Connection::open(&database).unwrap();
    let before = parent_rows(&connection, &parents);
    core.set_repository_visibility("owner", "demo", true)
        .unwrap();
    core.set_repository_family("owner", "demo", Some("temporary".into()))
        .unwrap();
    core.set_repository_family("owner", "demo", None).unwrap();
    core.update_pull_request(
        "owner",
        "demo",
        1,
        UpdatePullRequestRequest {
            body: Some("updated body".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let prepared = core
        .prepare_repository_transfer(transfer(parents.repository))
        .unwrap();
    core.commit_repository_transfer(
        prepared.transaction_id,
        serde_json::json!({"verified": true}),
    )
    .unwrap();
    assert_eq!(parent_rows(&connection, &parents), before);
    assert_external_evidence(&connection);
    let default: String = connection
        .query_row(
            "SELECT future_default FROM repositories WHERE id = ?1",
            [parents.repository.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(default, "preserved binding");
    assert_eq!(
        core.get_pull_request("destination", "demo", 1)
            .unwrap()
            .body
            .as_deref(),
        Some("updated body")
    );
    drop(connection);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        reopened.get_repository("destination", "demo").unwrap().id,
        parents.repository
    );
    assert!(
        reopened
            .get_repository("destination", "demo")
            .unwrap()
            .private
    );
    assert!(
        reopened
            .get_repository("destination", "demo")
            .unwrap()
            .family
            .is_none()
    );
    let connection = Connection::open(&database).unwrap();
    assert_eq!(parent_rows(&connection, &parents), before);
    assert_external_evidence(&connection);
}

#[test]
fn rejected_insert_restores_shared_and_durable_state_without_losing_evidence() {
    let (_directory, database, core, parents) = fixture();
    let other_handle = core.clone();
    let connection = Connection::open(&database).unwrap();
    let before = parent_rows(&connection, &parents);
    connection
        .execute_batch(
            "CREATE TRIGGER reject_new_check BEFORE INSERT ON check_runs
             WHEN NEW.name = 'rejected'
             BEGIN SELECT RAISE(ABORT, 'injected insert failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        core.create_check_run(
            "owner",
            "demo",
            CreateCheckRunRequest {
                name: "rejected".into(),
                head_sha: HEAD.into(),
                ..Default::default()
            }
        ),
        Err(ForgeError::Storage(ref message)) if message.contains("injected insert failure")
    ));
    assert_eq!(
        other_handle
            .list_check_runs("owner", "demo", Some(HEAD))
            .unwrap()
            .total_count,
        1
    );
    assert_eq!(parent_rows(&connection, &parents), before);
    assert_external_evidence(&connection);
    drop(connection);
    drop(other_handle);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        reopened
            .list_check_runs("owner", "demo", Some(HEAD))
            .unwrap()
            .total_count,
        1
    );
    assert_external_evidence(&Connection::open(&database).unwrap());
}

#[test]
fn failure_after_parent_update_rolls_back_the_entire_transfer_and_shared_state() {
    let (_directory, database, core, parents) = fixture();
    let prepared = core
        .prepare_repository_transfer(transfer(parents.repository))
        .unwrap();
    let other_handle = core.clone();
    let connection = Connection::open(&database).unwrap();
    let before = parent_rows(&connection, &parents);
    connection
        .execute_batch(
            "CREATE TRIGGER reject_transfer_commit BEFORE UPDATE ON repository_transfer_journal
             WHEN NEW.status = 'committed'
             BEGIN SELECT CASE
                 WHEN (SELECT owner FROM main.repositories WHERE id = NEW.repository_id) = 'destination'
                 THEN RAISE(ABORT, 'injected failure after parent update')
                 ELSE RAISE(ABORT, 'parent update was not observed')
             END; END;",
        )
        .unwrap();
    assert!(matches!(
        core.commit_repository_transfer(
            prepared.transaction_id,
            serde_json::json!({"verified": true})
        ),
        Err(ForgeError::Storage(ref message)) if message.contains("injected failure after parent update")
    ));
    assert_eq!(
        other_handle.get_repository("owner", "demo").unwrap().id,
        parents.repository
    );
    assert!(matches!(
        other_handle.get_repository("destination", "demo"),
        Err(ForgeError::NotFound(_))
    ));
    assert_eq!(
        other_handle.get_repository_transfer("transfer").unwrap(),
        prepared
    );
    let owner: String = connection
        .query_row(
            "SELECT owner FROM repositories WHERE id = ?1",
            [parents.repository.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(owner, "owner");
    assert_eq!(parent_rows(&connection, &parents), before);
    assert_external_evidence(&connection);
    drop(connection);
    drop(other_handle);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        reopened.get_repository("owner", "demo").unwrap().id,
        parents.repository
    );
    assert_eq!(
        reopened.get_repository_transfer("transfer").unwrap(),
        prepared
    );
    assert_external_evidence(&Connection::open(&database).unwrap());
}

#[test]
fn refused_credential_deletion_rolls_back_then_authorized_revocation_persists() {
    let (_directory, database, core, _parents) = fixture();
    let session = core
        .create_session("owner", "correct horse battery")
        .unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let token = core
        .create_personal_access_token(&actor, "retained", None)
        .unwrap();
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_token_delete BEFORE DELETE ON personal_access_tokens
             BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        core.revoke_personal_access_token("owner", token.token.id),
        Err(ForgeError::Storage(ref message)) if message.contains("injected delete failure")
    ));
    assert!(
        core.authenticate_personal_access_token(&token.secret)
            .is_some()
    );
    assert_external_evidence(&connection);
    drop(connection);
    drop(core);

    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert!(
        core.authenticate_personal_access_token(&token.secret)
            .is_some()
    );
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER reject_token_delete;")
        .unwrap();
    assert!(
        core.revoke_personal_access_token("owner", token.token.id)
            .unwrap()
    );
    assert!(
        core.authenticate_personal_access_token(&token.secret)
            .is_none()
    );
    let remaining: i64 = connection
        .query_row("SELECT COUNT(*) FROM personal_access_tokens", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 0);
    assert_external_evidence(&connection);
    drop(connection);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert!(
        reopened
            .authenticate_personal_access_token(&token.secret)
            .is_none()
    );
    assert_external_evidence(&Connection::open(&database).unwrap());
}

#[test]
fn intentional_owned_deletion_and_same_slug_recreation_do_not_restore_old_rows() {
    let (_directory, database, core, parents) = fixture();
    let survivor = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "survivor".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "INSERT INTO future_outbox VALUES ('survivor', ?1, 'retain me')",
            [survivor.id.to_string()],
        )
        .unwrap();
    core.delete_repository("owner", "demo").unwrap();
    assert!(matches!(
        core.get_repository("owner", "demo"),
        Err(ForgeError::NotFound(_))
    ));
    // This intentional parent deletion exercises the declared FK cascade. The
    // unrelated survivor and append-only audit must remain intact.
    let removed_receipts: i64 = connection
        .query_row("SELECT COUNT(*) FROM future_ci_receipts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(removed_receipts, 0);
    // Creation identity remains reserved even after its repository is removed.
    assert_creation_receipt(&connection);
    let survivor_payload: String = connection
        .query_row(
            "SELECT payload FROM future_outbox WHERE id = 'survivor'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(survivor_payload, "retain me");
    let replacement = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "demo".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(replacement.id, parents.repository);
    assert!(
        core.list_pull_requests("owner", "demo", None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(core.list_audit("owner/demo").unwrap().len(), 1);
    drop(connection);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        reopened.get_repository("owner", "demo").unwrap().id,
        replacement.id
    );
    assert_eq!(
        reopened.get_repository("owner", "survivor").unwrap().id,
        survivor.id
    );
    assert!(
        reopened
            .list_pull_requests("owner", "demo", None)
            .unwrap()
            .is_empty()
    );
    assert_eq!(reopened.list_audit("owner/demo").unwrap().len(), 1);
    assert_creation_receipt(&Connection::open(&database).unwrap());
}
