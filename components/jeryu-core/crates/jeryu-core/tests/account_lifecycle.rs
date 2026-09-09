use std::sync::{Arc, Barrier};

use chrono::{Duration, Utc};
use jeryu_core::{
    AccountStatus, CreateOrganizationRequest, CreateTeamRequest, ForgeCore, ForgeError,
    InvitationBindings, UserRole,
};
use rusqlite::{Connection, params};

fn user_bindings() -> InvitationBindings {
    InvitationBindings {
        role: UserRole::User,
        teams: Vec::new(),
    }
}

fn invite(core: &ForgeCore, login: &str) -> jeryu_core::AccountInvitationReceipt {
    core.create_account_invitation(
        "owner",
        login,
        "Canary User",
        user_bindings(),
        Utc::now() + Duration::hours(1),
    )
    .expect("create invitation")
}

fn activation_error(error: ForgeError) -> String {
    match error {
        ForgeError::Validation(message) => message,
        other => panic!("expected non-disclosing validation error, got {other}"),
    }
}

#[test]
fn canonical_logins_and_invitation_reservations_fail_closed() {
    let core = ForgeCore::new();
    assert!(matches!(
        core.create_account("Alice", "correct horse battery", UserRole::User),
        Err(ForgeError::Validation(_))
    ));
    assert!(matches!(
        core.create_account_invitation(
            "owner",
            "alice",
            "Alice",
            user_bindings(),
            Utc::now() + Duration::hours(25),
        ),
        Err(ForgeError::Validation(_))
    ));

    let first = invite(&core, "alice");
    assert_eq!(first.activation_secret.len(), 64);
    assert!(matches!(
        core.create_account_invitation(
            "owner",
            "alice",
            "Second Alice",
            user_bindings(),
            Utc::now() + Duration::hours(1),
        ),
        Err(ForgeError::Conflict(_))
    ));
    assert!(core.revoke_account_invitation(first.invitation.id).unwrap());
    assert!(invite(&core, "alice").invitation.revoked_at.is_none());
}

#[test]
fn invitation_failures_are_indistinguishable_and_attempts_are_bounded() {
    let core = ForgeCore::new();
    let invitation = invite(&core, "alice");
    let unknown = activation_error(
        core.start_account_activation("nobody", "not-a-secret")
            .unwrap_err(),
    );
    for _ in 0..5 {
        let wrong = activation_error(
            core.start_account_activation("alice", "not-a-secret")
                .unwrap_err(),
        );
        assert_eq!(wrong, unknown);
    }
    let exhausted = activation_error(
        core.start_account_activation("alice", &invitation.activation_secret)
            .unwrap_err(),
    );
    assert_eq!(exhausted, unknown);
    assert_eq!(core.list_account_invitations()[0].attempt_count, 5);
}

#[test]
fn expired_revoked_consumed_and_unknown_invitations_share_one_error() {
    let baseline = activation_error(
        ForgeCore::new()
            .start_account_activation("nobody", "not-a-secret")
            .unwrap_err(),
    );

    let revoked_core = ForgeCore::new();
    let revoked = invite(&revoked_core, "revoked");
    revoked_core
        .revoke_account_invitation(revoked.invitation.id)
        .unwrap();
    assert_eq!(
        activation_error(
            revoked_core
                .start_account_activation("revoked", &revoked.activation_secret)
                .unwrap_err()
        ),
        baseline
    );

    let consumed_core = ForgeCore::new();
    let consumed = invite(&consumed_core, "consumed");
    let challenge = consumed_core
        .start_account_activation("consumed", &consumed.activation_secret)
        .unwrap();
    consumed_core
        .complete_account_activation(&challenge.challenge, "correct horse battery")
        .unwrap();
    assert_eq!(
        activation_error(
            consumed_core
                .start_account_activation("consumed", &consumed.activation_secret)
                .unwrap_err()
        ),
        baseline
    );

    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");
    let expired = {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        invite(&core, "expired")
    };
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "UPDATE account_invitations SET expires_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
        params![expired.invitation.id.to_string()],
    )
    .unwrap();
    drop(conn);
    let expired_core = ForgeCore::open_sqlite(&db).unwrap();
    assert_eq!(
        activation_error(
            expired_core
                .start_account_activation("expired", &expired.activation_secret)
                .unwrap_err()
        ),
        baseline
    );
}

#[test]
fn activation_has_exactly_one_winner_and_requires_mfa_before_credentials() {
    let core = ForgeCore::new();
    let invitation = invite(&core, "alice");
    assert!(!format!("{invitation:?}").contains(&invitation.activation_secret));
    let start = core
        .start_account_activation("alice", &invitation.activation_secret)
        .expect("start activation");
    assert!(!format!("{start:?}").contains(&start.challenge));
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let worker = core.clone();
        let challenge = start.challenge.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            worker.complete_account_activation(&challenge, "correct horse battery")
        }));
    }
    barrier.wait();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("activation worker"))
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);

    let account = core.get_account("alice").expect("activated account");
    assert_eq!(account.status, AccountStatus::PendingMfa);
    assert!(
        core.authenticate_password("alice", "correct horse battery")
            .is_err()
    );
    assert!(core.create_session("alice").is_err());
    assert!(
        core.create_personal_access_token("alice", "cli", None)
            .is_err()
    );

    let active = core
        .mark_account_mfa_active("alice")
        .expect("first MFA enrollment activates account");
    assert_eq!(active.status, AccountStatus::Active);
    assert!(
        core.authenticate_password("alice", "correct horse battery")
            .is_ok()
    );
}

#[test]
fn disable_lock_and_reactivation_revoke_every_existing_credential() {
    let core = ForgeCore::new();
    let created = core
        .create_account("alice", "correct horse battery", UserRole::User)
        .unwrap();
    let session = core.create_session("alice").unwrap().token;
    let pat = core
        .create_personal_access_token("alice", "cli", None)
        .unwrap()
        .secret;

    let disabled = core.disable_account("alice").unwrap();
    assert_eq!(disabled.status, AccountStatus::Disabled);
    assert_eq!(disabled.auth_epoch, created.auth_epoch + 1);
    assert!(core.authenticate_session(&session).is_none());
    assert!(core.authenticate_personal_access_token(&pat).is_none());
    assert!(
        core.authenticate_password("alice", "correct horse battery")
            .is_err()
    );

    let pending = core.reactivate_account("alice").unwrap();
    assert_eq!(pending.status, AccountStatus::PendingMfa);
    assert!(core.create_session("alice").is_err());
    core.mark_account_mfa_active("alice").unwrap();
    let replacement = core.create_session("alice").unwrap().token;
    let locked = core.lock_account("alice").unwrap();
    assert_eq!(locked.status, AccountStatus::Locked);
    assert!(core.authenticate_session(&replacement).is_none());
}

#[test]
fn invitation_bindings_apply_only_when_activation_completes() {
    let core = ForgeCore::new();
    core.create_organization(CreateOrganizationRequest {
        login: "neverhuman".to_string(),
        display_name: None,
    })
    .unwrap();
    core.create_team(
        "neverhuman",
        CreateTeamRequest {
            name: "Contributors".to_string(),
            slug: Some("contributors".to_string()),
            members: Vec::new(),
        },
    )
    .unwrap();
    let receipt = core
        .create_account_invitation(
            "owner",
            "alice",
            "Alice",
            InvitationBindings {
                role: UserRole::User,
                teams: vec![
                    "neverhuman/contributors".to_string(),
                    "neverhuman/contributors".to_string(),
                ],
            },
            Utc::now() + Duration::hours(1),
        )
        .unwrap();
    assert!(core.list_teams("neverhuman").unwrap()[0].members.is_empty());
    let challenge = core
        .start_account_activation("alice", &receipt.activation_secret)
        .unwrap();
    core.complete_account_activation(&challenge.challenge, "correct horse battery")
        .unwrap();
    assert_eq!(
        core.list_teams("neverhuman").unwrap()[0].members,
        vec!["alice"]
    );
}

#[test]
fn invitation_secret_is_absent_from_sqlite_and_state_survives_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");
    let secret;
    {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let invitation = invite(&core, "alice");
        secret = invitation.activation_secret.clone();
        let challenge = core
            .start_account_activation("alice", &invitation.activation_secret)
            .unwrap();
        core.complete_account_activation(&challenge.challenge, "correct horse battery")
            .unwrap();
    }
    let bytes = std::fs::read(&db).unwrap();
    assert!(
        !bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()),
        "SQLite must never contain the activation secret"
    );
    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    assert_eq!(
        reopened.get_account("alice").unwrap().status,
        AccountStatus::PendingMfa
    );
    assert!(reopened.list_account_invitations()[0].consumed_at.is_some());
}

#[test]
fn first_owner_bootstrap_is_permanently_consumed() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");
    {
        let core = ForgeCore::open_sqlite(&db).unwrap();
        let invitation = core
            .create_bootstrap_owner_invitation(
                "owner",
                "First Owner",
                Utc::now() + Duration::hours(1),
            )
            .unwrap();
        let challenge = core
            .start_account_activation("owner", &invitation.activation_secret)
            .unwrap();
        let owner = core
            .complete_account_activation(&challenge.challenge, "correct horse battery")
            .unwrap();
        assert_eq!(owner.role, UserRole::Admin);
        assert_eq!(owner.status, AccountStatus::PendingMfa);
        assert!(core.bootstrap_owner_consumed());
    }
    let reopened = ForgeCore::open_sqlite(&db).unwrap();
    assert!(reopened.bootstrap_owner_consumed());
    assert!(matches!(
        reopened.create_bootstrap_owner_invitation(
            "replacement",
            "Replacement",
            Utc::now() + Duration::hours(1),
        ),
        Err(ForgeError::Conflict(_))
    ));
}

#[test]
fn sqlite_open_rejects_case_fold_collisions() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("forge.sqlite");
    drop(ForgeCore::open_sqlite(&db).unwrap());
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
    for login in ["Alice", "alice"] {
        conn.execute(
            r#"
            INSERT INTO user_accounts (
              login, display_name, password_hash, role, status, auth_epoch,
              must_change_password, created_at, updated_at
            ) VALUES (?1, ?1, '$argon2id$invalid', 'user', 'active', 0, 0,
                      '2026-08-25T00:00:00Z', '2026-08-25T00:00:00Z')
            "#,
            params![login],
        )
        .unwrap();
    }
    drop(conn);
    let error = ForgeCore::open_sqlite(&db).unwrap_err().to_string();
    assert!(error.contains("case-fold collision"), "{error}");
}
