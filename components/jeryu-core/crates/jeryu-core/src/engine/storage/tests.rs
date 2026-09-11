use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::{Duration, Utc};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use uuid::Uuid;

use super::{load_state, snapshot, stage_state};
use crate::{AccountInvitationReceipt, ForgeCore, ForgeError, InvitationBindings, UserRole};

struct InvitationFixture {
    _directory: TempDir,
    database: PathBuf,
    core: ForgeCore,
    id: Uuid,
    original_row: (i64, Vec<u8>),
}

fn private_directory() -> TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

fn invite(core: &ForgeCore, login: &str) -> AccountInvitationReceipt {
    core.create_account_invitation(
        "owner",
        login,
        "Invited user",
        InvitationBindings {
            role: UserRole::User,
            teams: Vec::new(),
        },
        Utc::now() + Duration::hours(1),
    )
    .unwrap()
}

fn invitation_row(connection: &Connection, id: Uuid) -> (i64, Vec<u8>) {
    connection
        .query_row(
            "SELECT rowid, future_opaque FROM main.account_invitations WHERE id = ?1",
            [id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn fixture(expired: bool) -> InvitationFixture {
    let directory = private_directory();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let id = invite(&core, "invited").invitation.id;
    drop(core);
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             ALTER TABLE account_invitations ADD COLUMN future_opaque BLOB;
             CREATE TABLE future_invitation_receipts (
                 invitation_id TEXT PRIMARY KEY REFERENCES account_invitations(id) ON DELETE CASCADE,
                 payload TEXT NOT NULL
             );",
        )
        .unwrap();
    connection
        .execute(
            "UPDATE account_invitations SET future_opaque = X'001180ff' WHERE id = ?1",
            [id.to_string()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO future_invitation_receipts VALUES (?1, 'retain this receipt')",
            [id.to_string()],
        )
        .unwrap();
    if expired {
        connection
            .execute(
                "UPDATE account_invitations SET expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
                [id.to_string()],
            )
            .unwrap();
    }
    let original_row = invitation_row(&connection, id);
    drop(connection);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    InvitationFixture {
        _directory: directory,
        database,
        core,
        id,
        original_row,
    }
}

fn assert_evidence(connection: &Connection, id: Uuid, original: &(i64, Vec<u8>)) {
    assert_eq!(&invitation_row(connection, id), original);
    let payload: String = connection
        .query_row(
            "SELECT payload FROM main.future_invitation_receipts WHERE invitation_id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, "retain this receipt");
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}

#[test]
fn serialized_successor_before_expired_row_preserves_live_login_constraint() {
    let fixture = fixture(true);
    let mut desired = fixture.core.runtime.state.read().clone();
    let now = Utc::now();
    let previous = desired.invitations.get_mut(&fixture.id).unwrap();
    assert!(previous.expires_at < now);
    previous.revoked_at = Some(now);
    let mut successor = previous.clone();
    successor.id = Uuid::new_v4();
    successor.activation_secret_hash = "f".repeat(64);
    successor.created_at = now;
    successor.expires_at = now + Duration::hours(1);
    successor.revoked_at = None;
    let successor_id = successor.id;
    desired.invitations.insert(successor.id, successor);
    drop(fixture.core);

    let mut connection = Connection::open(&fixture.database).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    let transaction = connection.transaction().unwrap();
    snapshot::create_tables(&transaction).unwrap();
    stage_state(&transaction, &desired).unwrap();
    // Reorder the real serializer output, rather than depending on randomized
    // HashMap traversal. The new active invitation must occupy the first row.
    transaction
        .execute_batch(
            "CREATE TEMP TABLE ordered_invitations AS SELECT * FROM temp.account_invitations;
             DELETE FROM temp.account_invitations;
             INSERT INTO temp.account_invitations
                 SELECT * FROM temp.ordered_invitations
                 ORDER BY revoked_at IS NOT NULL, id;",
        )
        .unwrap();
    let first_id: String = transaction
        .query_row(
            "SELECT id FROM temp.account_invitations ORDER BY rowid LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(first_id, successor_id.to_string());
    snapshot::apply(&transaction).unwrap();
    transaction.commit().unwrap();
    assert_evidence(&connection, fixture.id, &fixture.original_row);
    let live_id: String = connection
        .query_row(
            "SELECT id FROM main.account_invitations WHERE canonical_login = 'invited'
             AND consumed_at IS NULL AND revoked_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(live_id, successor_id.to_string());
    drop(connection);

    let reopened = ForgeCore::open_sqlite(&fixture.database).unwrap();
    assert_eq!(
        reopened.runtime.state.read().invitations,
        desired.invitations
    );
    assert_evidence(
        &Connection::open(&fixture.database).unwrap(),
        fixture.id,
        &fixture.original_row,
    );
}

#[test]
fn public_expired_invitation_reissue_reopens_with_both_receipts() {
    let fixture = fixture(true);
    let successor = invite(&fixture.core, "invited");
    let expected = fixture.core.list_account_invitations();
    assert_eq!(expected.len(), 2);
    assert!(
        expected
            .iter()
            .find(|invitation| invitation.id == fixture.id)
            .unwrap()
            .revoked_at
            .is_some()
    );
    assert!(successor.invitation.revoked_at.is_none());
    assert_evidence(
        &Connection::open(&fixture.database).unwrap(),
        fixture.id,
        &fixture.original_row,
    );
    drop(fixture.core);
    let reopened = ForgeCore::open_sqlite(&fixture.database).unwrap();
    assert_eq!(reopened.list_account_invitations(), expected);
    assert_evidence(
        &Connection::open(&fixture.database).unwrap(),
        fixture.id,
        &fixture.original_row,
    );
}

#[test]
fn invalid_final_invitation_uniqueness_rolls_back_existing_updates_and_shared_state() {
    let fixture = fixture(false);
    let other_handle = fixture.core.clone();
    let previous = fixture.core.list_account_invitations();
    let result = fixture.core.coordinator().with_authority(&[], || {
        let mut state = fixture.core.runtime.state.write();
        let previous_state = state.clone();
        let existing = state.invitations.get_mut(&fixture.id).unwrap();
        existing.display_name = "This update must roll back".into();
        let mut conflicting = existing.clone();
        conflicting.id = Uuid::new_v4();
        conflicting.activation_secret_hash = "e".repeat(64);
        // Keep both rows active with the same login: this final state is invalid.
        state.invitations.insert(conflicting.id, conflicting);
        fixture
            .core
            .persist_after_mutation(&mut state, previous_state)
    });
    assert!(matches!(
        result,
        Err(ForgeError::Storage(ref message))
            if message.contains("UNIQUE constraint failed: account_invitations.canonical_login")
    ));
    assert_eq!(other_handle.list_account_invitations(), previous);
    let connection = Connection::open(&fixture.database).unwrap();
    let durable = load_state(&connection).unwrap();
    assert_eq!(
        durable.invitations,
        fixture.core.runtime.state.read().invitations
    );
    assert_evidence(&connection, fixture.id, &fixture.original_row);
    drop(connection);
    drop(other_handle);
    drop(fixture.core);
    let reopened = ForgeCore::open_sqlite(&fixture.database).unwrap();
    assert_eq!(reopened.list_account_invitations(), previous);
    assert_evidence(
        &Connection::open(&fixture.database).unwrap(),
        fixture.id,
        &fixture.original_row,
    );
}

#[test]
fn changed_existing_row_preserves_future_not_null_column_without_default() {
    let directory = private_directory();
    let database = directory.path().join("forge.sqlite");
    drop(ForgeCore::open_sqlite(&database).unwrap());
    let source = ForgeCore::new();
    let invitation = invite(&source, "invited");
    let state = source.runtime.state.read().clone();
    let connection = Connection::open(&database).unwrap();
    // This future schema is created while the table is empty. Its independent
    // owner supplies the initial value; old Core must preserve it on UPDATE.
    connection
        .execute_batch("ALTER TABLE account_invitations ADD COLUMN future_required BLOB NOT NULL;")
        .unwrap();
    snapshot::create_tables(&connection).unwrap();
    stage_state(&connection, &state).unwrap();
    connection
        .execute_batch(
            "INSERT INTO main.account_invitations
             SELECT desired.*, X'001180ff' FROM temp.account_invitations AS desired;",
        )
        .unwrap();
    let original_rowid: i64 = connection
        .query_row("SELECT rowid FROM main.account_invitations", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(connection);
    drop(source);

    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert!(
        core.revoke_account_invitation(invitation.invitation.id)
            .unwrap()
    );
    let previous = core.list_account_invitations();
    assert!(matches!(
        core.create_account_invitation(
            "owner",
            "new-user",
            "New user",
            InvitationBindings { role: UserRole::User, teams: Vec::new() },
            Utc::now() + Duration::hours(1),
        ),
        Err(ForgeError::Storage(ref message))
            if message.contains("NOT NULL constraint failed: account_invitations.future_required")
    ));
    assert_eq!(core.list_account_invitations(), previous);
    drop(core);

    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(reopened.list_account_invitations(), previous);
    let connection = Connection::open(&database).unwrap();
    let retained: (i64, Vec<u8>) = connection
        .query_row(
            "SELECT rowid, future_required FROM main.account_invitations WHERE id = ?1",
            params![invitation.invitation.id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(retained, (original_rowid, vec![0_u8, 17, 128, 255]));
}
