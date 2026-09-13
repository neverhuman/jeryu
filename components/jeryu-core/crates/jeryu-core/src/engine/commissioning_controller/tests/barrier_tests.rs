//! Admission controls use actual public Core writers and isolated SQLite/files.
//! The commissioning trust and completion evidence remain test fixtures.

use super::*;
use crate::{CreateCheckRunRequest, CreatePullRequestRequest, RepoMaterializer};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration as WaitDuration, Instant};

fn blocked<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(&result, Err(ForgeError::WriterUnavailable(message)) if message.contains("commissioning")),
        "{result:?}"
    );
}

fn create(core: &ForgeCore, name: &str) -> Result<crate::Repository> {
    core.create_repository(
        "owner",
        CreateRepositoryRequest {
            name: name.into(),
            ..Default::default()
        },
    )
}

fn check(core: &ForgeCore) -> Result<crate::CheckRun> {
    core.create_check_run(
        "owner",
        "fixture",
        CreateCheckRunRequest {
            name: "fixture/required".into(),
            head_sha: "a".repeat(40),
            ..Default::default()
        },
    )
}

#[test]
fn read_only_database_still_checks_commissioning_before_a_noop() {
    let fixture = Fixture::new();
    let operation = fixture.reserve();
    let database = fixture.directory.path().join("forge.sqlite");
    let before = std::fs::read(&database).unwrap();
    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o400)).unwrap();

    // The existing repository is already private. Its no-op does not need a
    // write, but must still observe and refuse the active operation barrier.
    blocked(
        fixture
            .core
            .set_repository_visibility("owner", "fixture", true),
    );
    assert_eq!(std::fs::read(&database).unwrap(), before);
    assert!(
        fixture
            .core
            .get_repository("owner", "fixture")
            .unwrap()
            .private
    );

    std::fs::set_permissions(&database, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(operation.id)
            .unwrap(),
        operation
    );
}

#[test]
fn every_ordinary_core_scope_refuses_before_state_or_external_effect() {
    let mut fixture = Fixture::new();
    create(&fixture.core, "unrelated").unwrap();
    let pull = fixture
        .core
        .create_pull_request(
            "owner",
            "fixture",
            "operator",
            CreatePullRequestRequest {
                title: "retained contribution".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some("a".repeat(40)),
                ..Default::default()
            },
        )
        .unwrap();
    fixture.attach();
    let operation = fixture
        .core
        .reserve_commissioning_operation(
            &fixture.operator,
            fixture.scope.repository_id,
            fixture.scope.contract_id,
            fixture.scope.request.clone(),
        )
        .unwrap();

    blocked(create(&fixture.core, "denied"));
    blocked(fixture.core.ensure_user("denied-profile"));
    blocked(check(&fixture.core));
    blocked(
        fixture
            .core
            .set_repository_readme("owner", "unrelated", "denied".into()),
    );
    blocked(
        fixture
            .core
            .set_repository_visibility("owner", "fixture", false),
    );
    blocked(
        fixture
            .core
            .revoke_personal_access_token("operator", fixture.operator_token),
    );
    blocked(fixture.core.grant_repo_access(
        "owner",
        "recorder",
        "owner",
        "fixture",
        RepoAccessLevel::Admin,
    ));
    blocked(fixture.core.block_repository_mutations(
        fixture.scope.repository_id,
        crate::RepositoryMutationBlock::ReadOnly {
            reason: "denied".into(),
            evidence: "denied".into(),
        },
    ));
    blocked(fixture.core.append_audit(
        "fixture.denied",
        "owner/fixture",
        "requested",
        serde_json::json!({}),
    ));
    blocked(fixture.core.create_review_challenge(
        &fixture.operator,
        fixture.scope.repository_id,
        pull.number,
        &"a".repeat(40),
    ));
    blocked(fixture.core.reserve_required_attempt(
        &fixture.operator,
        fixture.scope.repository_id,
        crate::ReserveRequiredAttemptRequest {
            publisher_id: Uuid::new_v4(),
            expected_head_sha: Some("a".repeat(40)),
            expected_tree_sha: Some("b".repeat(40)),
            context: "fixture/required".into(),
            idempotency_key: Some("denied".into()),
            expires_at: Utc::now() + Duration::seconds(60),
        },
    ));
    // Verify rejection precedes the actual shared callbacks, including source,
    // pull and transfer scopes whose public preconditions vary by operation.
    let called = std::cell::Cell::new(false);
    let effect = || {
        called.set(true);
        Ok(())
    };
    blocked(
        fixture
            .core
            .with_profile_mutation("owner", "fixture", "new-profile", effect),
    );
    blocked(fixture.core.with_source_mutation(
        "owner",
        "fixture",
        Some("owner/unrelated"),
        "operator",
        effect,
    ));
    blocked(
        fixture
            .core
            .with_pull_mutation("owner", "fixture", pull.number, effect),
    );
    blocked(
        fixture
            .core
            .with_repository_id_mutation(fixture.scope.repository_id, effect),
    );
    blocked(fixture.core.with_transfer_prepare_mutation(
        fixture.scope.repository_id,
        "denied",
        "denied",
        effect,
    ));
    assert!(!called.get());
    assert!(fixture.core.get_repository("owner", "denied").is_err());
    assert!(
        fixture
            .core
            .get_repository("owner", "fixture")
            .unwrap()
            .private
    );
    assert!(
        fixture
            .core
            .get_repository_readme("owner", "unrelated")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .core
            .list_check_runs("owner", "fixture", None)
            .unwrap()
            .total_count,
        0
    );
    assert!(fixture.core.list_audit("owner/fixture").unwrap().is_empty());
    assert_eq!(
        fixture
            .core
            .commissioning_operation(&fixture.operator, fixture.address(&operation))
            .unwrap(),
        operation
    );
    assert!(fixture.core.required_publisher_custody().is_ok());
}

#[derive(Debug)]
struct HeldMaterializer {
    file: PathBuf,
    entered: mpsc::Sender<()>,
    release: Arc<Barrier>,
}

impl RepoMaterializer for HeldMaterializer {
    fn materialize(&self, _: &str, _: &str, _: &str) -> Result<()> {
        self.entered.send(()).unwrap();
        self.release.wait();
        std::fs::write(&self.file, b"actual guarded filesystem effect").unwrap();
        Ok(())
    }
}

#[test]
fn reservation_drains_an_actual_inflight_materializer_and_then_denies_new_writers() {
    let mut fixture = Fixture::new();
    fixture.attach();
    let (entered, receiver) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let file = fixture.directory.path().join("materialized");
    let writer = fixture
        .core
        .clone()
        .with_repo_materializer(Arc::new(HeldMaterializer {
            file: file.clone(),
            entered,
            release: release.clone(),
        }));
    let write = std::thread::spawn(move || create(&writer, "before-reservation"));
    receiver.recv_timeout(WaitDuration::from_secs(5)).unwrap();
    let admission_before = fixture.core.coordinator().admission_entries();
    let reserving = fixture.core.clone();
    let operator = fixture.operator.clone();
    let scope = fixture.scope.clone();
    let (sender, result) = mpsc::channel();
    let reserve = std::thread::spawn(move || {
        sender
            .send(reserving.reserve_commissioning_operation(
                &operator,
                scope.repository_id,
                scope.contract_id,
                scope.request,
            ))
            .unwrap();
    });
    let deadline = Instant::now() + WaitDuration::from_secs(5);
    while fixture.core.coordinator().admission_entries() == admission_before {
        assert!(
            Instant::now() < deadline,
            "reservation did not reach the authority guard"
        );
        std::thread::yield_now();
    }
    assert!(matches!(result.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap()
            .is_none()
    );
    assert!(!file.exists());
    release.wait();
    write.join().unwrap().unwrap();
    let operation = result
        .recv_timeout(WaitDuration::from_secs(5))
        .unwrap()
        .unwrap();
    reserve.join().unwrap();
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"actual guarded filesystem effect"
    );
    blocked(create(&fixture.core, "after-reservation"));
    blocked(check(&fixture.core));
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(operation.id)
            .unwrap(),
        operation
    );
}

#[test]
fn reopened_barrier_skips_schema_and_state_backfills_and_reattaches_readback_authority() {
    let mut fixture = Fixture::new();
    fixture
        .core
        .create_pull_request(
            "owner",
            "fixture",
            "operator",
            CreatePullRequestRequest {
                title: "retained pre-migration pull".into(),
                head: "topic".into(),
                base: "main".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let token = fixture
        .core
        .create_personal_access_token(&fixture.operator, "reopen", None)
        .unwrap();
    fixture.operator = fixture
        .core
        .authenticate_actor(ActorCredential::PersonalAccessToken(&token.secret))
        .unwrap();
    fixture.scope.operator = fixture
        .core
        .validate_actor_locked(&fixture.core.runtime.state.read(), &fixture.operator, true)
        .unwrap();
    let operation = fixture.reserve();
    let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    // Deliberately remove fixture protection so the normal open-time backfill
    // would be visible. A trigger also catches an attempted migration UPDATE.
    conn.execute_batch("DELETE FROM branch_protection_rules; UPDATE pull_requests SET source_repository = ''; CREATE TRIGGER fixture_no_backfill BEFORE UPDATE ON pull_requests BEGIN SELECT RAISE(ABORT, 'migration must not execute'); END;").unwrap();
    let db_before = std::fs::read(fixture.directory.path().join("forge.sqlite")).unwrap();
    let Fixture {
        directory,
        core,
        scope,
        ..
    } = fixture;
    drop(core);
    let core = ForgeCore::open_managed(
        directory.path().join("forge.sqlite"),
        directory.path().join("git"),
    )
    .unwrap();
    // Reauthenticate a retained credential; no session or token is issued while
    // the restarted runtime holds ordinary admission closed.
    let operator = core
        .authenticate_actor(ActorCredential::PersonalAccessToken(&token.secret))
        .unwrap();
    assert_eq!(
        core.runtime
            .storage
            .as_ref()
            .unwrap()
            .commissioning_operation(operation.id)
            .unwrap(),
        operation
    );
    assert!(
        core.get_branch_protection("owner", "fixture", "main")
            .is_err()
    );
    blocked(check(&core));
    assert_eq!(
        std::fs::read(directory.path().join("forge.sqlite")).unwrap(),
        db_before
    );
    let authority = Arc::new(FixtureAuthority {
        custody: core.required_publisher_custody().unwrap(),
        scope: scope.clone(),
        recorder: scope.operator.clone(),
        tamper_action: AtomicU8::new(0),
    });
    let core = core.with_commissioning_authority(authority).unwrap();
    assert!(core.required_publisher_custody().is_ok());
    let address = CommissioningOperationAddress {
        repository_id: scope.repository_id,
        contract_id: scope.contract_id,
        operation_id: operation.id,
    };
    assert_eq!(
        core.commissioning_operation(&operator, address).unwrap(),
        operation
    );
}

#[test]
fn only_verified_in_window_close_reopens_ordinary_writers() {
    let mut fixture = Fixture::new();
    fixture.attach();
    let mut operation = fixture.reserve();
    while operation.next_step(Utc::now()).unwrap().is_some() {
        operation = fixture.complete(&fixture.admit(&operation));
    }
    blocked(check(&fixture.core));
    let address = fixture.address(&operation);
    let closed = fixture
        .core
        .close_commissioning_operation(
            &fixture.operator,
            address,
            operation.current,
            b"fixture completed restoration".to_vec(),
        )
        .unwrap();
    assert!(!closed.blocks_admission());
    check(&fixture.core).unwrap();
    assert_eq!(
        fixture
            .core
            .list_check_runs("owner", "fixture", None)
            .unwrap()
            .total_count,
        1
    );
}

#[test]
fn failed_or_expired_effects_retain_barrier_but_permit_outcome_readback() {
    for late in [false, true] {
        let mut fixture = Fixture::new();
        fixture.attach();
        let pending = fixture.admit(&fixture.reserve());
        let mut completion = Fixture::completion(&pending);
        if !late {
            completion.outcome = CommissioningEffectOutcome::Failed;
        }
        let now = if late {
            fixture.scope.expires_at
        } else {
            Utc::now()
        };
        let failed = fixture
            .store()
            .complete_commissioning_step(
                pending.id,
                &completion,
                &fixture.scope.operator,
                CommissioningRecordingAuthority::CurrentOperator,
                now,
            )
            .unwrap();
        assert!(failed.recovery_required && failed.blocks_admission());
        blocked(check(&fixture.core));
        blocked(create(&fixture.core, "denied"));
        assert_eq!(
            fixture
                .core
                .commissioning_operation(&fixture.recorder, fixture.address(&failed))
                .unwrap(),
            failed
        );
    }
}

#[test]
fn corrupt_projection_cannot_authorize_an_ordinary_effect() {
    let fixture = Fixture::new();
    let operation = fixture.reserve();
    let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    assert!(
        conn.execute(
            "UPDATE forge_commissioning_operations SET closed = 1 WHERE id = ?1",
            params![operation.id.to_string()]
        )
        .is_err()
    );
    // Damage only isolated test storage after proving the installed constraint
    // refuses it. A forged closed bit must not bypass chain reconstruction.
    conn.execute_batch("DROP TRIGGER forge_commissioning_operation_guard;")
        .unwrap();
    conn.execute(
        "UPDATE forge_commissioning_operations SET closed = 1 WHERE id = ?1",
        params![operation.id.to_string()],
    )
    .unwrap();
    blocked(check(&fixture.core));
    blocked(create(&fixture.core, "denied"));
    blocked(fixture.store().persist(&fixture.core.runtime.state.read()));
    assert_eq!(
        fixture
            .core
            .list_check_runs("owner", "fixture", None)
            .unwrap()
            .total_count,
        0
    );
    assert!(fixture.core.get_repository("owner", "denied").is_err());
}

#[test]
fn partial_schema_or_orphaned_records_refuse_live_writes_and_reopen_without_migration() {
    for damage in ["operations-table", "records-table", "orphaned-record"] {
        let fixture = Fixture::new();
        fixture.reserve();
        let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        assert!(
            conn.execute("DELETE FROM forge_commissioning_operations", [])
                .is_err()
        );
        assert!(
            conn.execute("DELETE FROM forge_commissioning_records", [])
                .is_err()
        );
        // Deliberately corrupt isolated storage only after proving both retained
        // production deletion constraints. Keep every remaining byte as evidence.
        conn.execute_batch("PRAGMA foreign_keys = OFF; DELETE FROM branch_protection_rules;")
            .unwrap();
        match damage {
            "operations-table" => conn.execute_batch("DROP TRIGGER forge_commissioning_operation_no_delete; DROP TABLE forge_commissioning_operations;").unwrap(),
            "records-table" => conn.execute_batch("DROP TRIGGER forge_commissioning_record_no_delete; DROP TABLE forge_commissioning_records;").unwrap(),
            "orphaned-record" => conn.execute_batch("DROP TRIGGER forge_commissioning_operation_no_delete; DELETE FROM forge_commissioning_operations;").unwrap(),
            _ => unreachable!(),
        }
        let before = std::fs::read(fixture.directory.path().join("forge.sqlite")).unwrap();
        blocked(check(&fixture.core));
        blocked(create(&fixture.core, "denied"));
        blocked(fixture.store().persist(&fixture.core.runtime.state.read()));
        let Fixture {
            directory, core, ..
        } = fixture;
        drop(core);
        blocked(ForgeCore::open_managed(
            directory.path().join("forge.sqlite"),
            directory.path().join("git"),
        ));
        assert_eq!(
            std::fs::read(directory.path().join("forge.sqlite")).unwrap(),
            before,
            "{damage}"
        );
        let remaining_table = if damage == "records-table" {
            "SELECT COUNT(*) FROM forge_commissioning_operations"
        } else {
            "SELECT COUNT(*) FROM forge_commissioning_records"
        };
        let retained: u64 = conn
            .query_row(remaining_table, [], |row| row.get(0))
            .unwrap();
        assert_eq!(retained, 1, "{damage}");
        let protections: u64 = conn
            .query_row("SELECT COUNT(*) FROM branch_protection_rules", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(protections, 0, "{damage}");
    }
}
