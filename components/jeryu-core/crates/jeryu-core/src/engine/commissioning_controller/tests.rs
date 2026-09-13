//! Real SQLite persistence and opaque-credential fixtures. None is an enrolled
//! commissioning operator, production verifier or installed effect dispatcher.

use super::*;
use crate::{ActorCredential, CreateRepositoryRequest, RepoAccessLevel, UserRole};
use chrono::Duration;
use rusqlite::{Connection, params};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Barrier};

mod barrier_tests;

struct Fixture {
    directory: tempfile::TempDir,
    core: ForgeCore,
    operator: AuthenticatedActor,
    recorder: AuthenticatedActor,
    operator_token: Uuid,
    scope: CommissioningRestoreScope,
}

fn actor(core: &ForgeCore, login: &str) -> (AuthenticatedActor, Uuid) {
    let password = "actual fixture password 5130490501483594442";
    core.create_account(login, password, UserRole::User)
        .unwrap();
    let session = core.create_session(login, password).unwrap();
    let owner = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let token = core
        .create_personal_access_token(&owner, "fixture", None)
        .unwrap();
    (
        core.authenticate_actor(ActorCredential::PersonalAccessToken(&token.secret))
            .unwrap(),
        token.token.id,
    )
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = directory.path().join("git");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let core = ForgeCore::open_managed(directory.path().join("forge.sqlite"), &root).unwrap();
        let repo = core
            .create_repository(
                "owner",
                CreateRepositoryRequest {
                    name: "fixture".into(),
                    private: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let (operator, operator_token) = actor(&core, "operator");
        let (recorder, _) = actor(&core, "recorder");
        for login in ["operator", "recorder"] {
            core.grant_repo_access(
                "fixture-owner",
                login,
                "owner",
                "fixture",
                RepoAccessLevel::Write,
            )
            .unwrap();
        }
        let binding = core
            .validate_actor_locked(&core.runtime.state.read(), &operator, true)
            .unwrap();
        let source = |digit: char| CommissioningSource {
            commit_sha: digit.to_string().repeat(40),
            tree_sha: "e".repeat(40),
        };
        let scope = CommissioningRestoreScope {
            contract_id: Uuid::new_v4(),
            repository_id: repo.id,
            backing_pair_id: Uuid::new_v4(),
            request: CommissioningRestoreRequest {
                expected_contract_sha256: "a".repeat(64),
                expected_snapshot_sha256: "b".repeat(64),
                idempotency_key: "fixture-operation".into(),
                kind: CommissioningRestoreKind::RestorePreAcceptanceP,
                before_manifest_sha256: "c".repeat(64),
                desired_manifest_sha256: "d".repeat(64),
            },
            operator: binding,
            p: source('1'),
            b: source('2'),
            c: source('3'),
            observed_main: source('2'),
            installation_inventory_sha256: "e".repeat(64),
            recovery_inventory_sha256: "f".repeat(64),
            desired_present: [true; 13],
            not_before: Utc::now() - Duration::seconds(1),
            expires_at: Utc::now() + Duration::seconds(600),
        };
        Self {
            directory,
            core,
            operator,
            recorder,
            operator_token,
            scope,
        }
    }
    fn store(&self) -> &super::super::storage::SqliteStore {
        self.core.runtime.storage.as_ref().unwrap()
    }
    fn reserve(&self) -> CommissioningRestoreOperation {
        self.store()
            .reserve_commissioning_restore(&self.scope, Utc::now())
            .unwrap()
    }
    fn admit(&self, operation: &CommissioningRestoreOperation) -> CommissioningRestoreOperation {
        self.store()
            .admit_commissioning_step(
                operation.id,
                &CommissioningStepRequest {
                    expected: operation.current.clone(),
                    step: operation.next_step(Utc::now()).unwrap().unwrap(),
                    local_custody_sha256: "a".repeat(64),
                },
                &self.scope.operator,
                Utc::now(),
            )
            .unwrap()
    }
    fn completion(operation: &CommissioningRestoreOperation) -> CommissioningCompletionRequest {
        CommissioningCompletionRequest {
            expected: operation.current.clone(),
            step_id: operation.steps.last().unwrap().id,
            outcome: CommissioningEffectOutcome::Verified,
            evidence: b"fixture receiving bytes".to_vec(),
        }
    }
    fn complete(&self, operation: &CommissioningRestoreOperation) -> CommissioningRestoreOperation {
        self.store()
            .complete_commissioning_step(
                operation.id,
                &Self::completion(operation),
                &self.scope.operator,
                CommissioningRecordingAuthority::CurrentOperator,
                Utc::now(),
            )
            .unwrap()
    }
    fn address(&self, operation: &CommissioningRestoreOperation) -> CommissioningOperationAddress {
        CommissioningOperationAddress {
            repository_id: self.scope.repository_id,
            contract_id: self.scope.contract_id,
            operation_id: operation.id,
        }
    }
    fn attach(&mut self) -> Arc<FixtureAuthority> {
        let recorder = self
            .core
            .validate_actor_locked(&self.core.runtime.state.read(), &self.recorder, true)
            .unwrap();
        let authority = Arc::new(FixtureAuthority {
            custody: self.core.required_publisher_custody().unwrap(),
            scope: self.scope.clone(),
            recorder,
            tamper_action: AtomicU8::new(0),
        });
        self.core = self
            .core
            .clone()
            .with_commissioning_authority(authority.clone())
            .unwrap();
        authority
    }
}

#[derive(Debug)]
struct FixtureAuthority {
    custody: RequiredPublisherCustody,
    scope: CommissioningRestoreScope,
    recorder: ReviewActorBinding,
    tamper_action: AtomicU8,
}
impl CommissioningAuthority for FixtureAuthority {
    fn custody(&self) -> &RequiredPublisherCustody {
        &self.custody
    }
    fn reserve(
        &self,
        actual: &RequiredPublisherCustody,
        repository_id: Uuid,
        contract_id: Uuid,
        actor: &ReviewActorBinding,
        request: &CommissioningRestoreRequest,
        _: DateTime<Utc>,
    ) -> Result<CommissioningRestoreScope> {
        if actual != &self.custody
            || repository_id != self.scope.repository_id
            || contract_id != self.scope.contract_id
            || actor != &self.scope.operator
            || request != &self.scope.request
        {
            return Err(ForgeError::Forbidden("fixture exact scope differs".into()));
        }
        Ok(self.scope.clone())
    }
    fn authorize(
        &self,
        actual: &RequiredPublisherCustody,
        operation: &CommissioningRestoreOperation,
        actor: &ReviewActorBinding,
        action: CommissioningAction<'_>,
        _: DateTime<Utc>,
    ) -> Result<CommissioningRecordingAuthority> {
        if actual != &self.custody || operation.scope != self.scope {
            return Err(ForgeError::Forbidden(
                "fixture actual custody or scope differs".into(),
            ));
        }
        let selected = match action {
            CommissioningAction::AdmitStep(_) => 1,
            CommissioningAction::RecordOutcome(_) => 2,
            CommissioningAction::Close { .. } => 3,
            CommissioningAction::Readback => 0,
        };
        if selected != 0 && self.tamper_action.load(Ordering::SeqCst) == selected {
            // Still an otherwise safe writer-root mode, but no longer the
            // exact enrolled full custody. R1 would commit before refusing.
            std::fs::set_permissions(
                &self.custody.storage_root.path,
                std::fs::Permissions::from_mode(0o750),
            )
            .unwrap();
        }
        if actor == &self.scope.operator {
            return Ok(CommissioningRecordingAuthority::CurrentOperator);
        }
        if actor == &self.recorder
            && matches!(
                action,
                CommissioningAction::Readback | CommissioningAction::RecordOutcome(_)
            )
        {
            return Ok(CommissioningRecordingAuthority::RecoveryRecorder);
        }
        Err(ForgeError::Forbidden(
            "fixture actor has no such commissioning scope".into(),
        ))
    }
}

#[test]
fn whole_operation_barrier_survives_completed_steps_snapshot_save_and_reopen() {
    let fixture = Fixture::new();
    let operation = fixture.complete(&fixture.admit(&fixture.reserve()));
    assert!(!operation.recovery_required);
    assert!(
        operation
            .steps
            .last()
            .unwrap()
            .completion
            .as_ref()
            .unwrap()
            .authorizes_progress
    );
    assert!(operation.blocks_admission());
    assert!(matches!(
        fixture.store().persist(&fixture.core.runtime.state.read()),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert_eq!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap(),
        Some(operation.clone())
    );
    let mut second = fixture.scope.clone();
    second.request.idempotency_key = "second".into();
    assert!(matches!(
        fixture
            .store()
            .reserve_commissioning_restore(&second, Utc::now()),
        Err(ForgeError::Conflict(_))
    ));
    let Fixture {
        directory, core, ..
    } = fixture;
    drop(core);
    let reopened = ForgeCore::open_managed(
        directory.path().join("forge.sqlite"),
        directory.path().join("git"),
    )
    .unwrap();
    assert_eq!(
        reopened
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .commissioning_barrier(operation.scope.backing_pair_id)
            .unwrap(),
        Some(operation)
    );
}

#[test]
fn two_real_sqlite_reservations_cannot_own_one_pair() {
    let fixture = Fixture::new();
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for n in 0..2 {
        let store = fixture.store().clone();
        let mut scope = fixture.scope.clone();
        scope.request.idempotency_key = format!("racer-{n}");
        let ready = barrier.clone();
        handles.push(std::thread::spawn(move || {
            ready.wait();
            store.reserve_commissioning_restore(&scope, Utc::now())
        }));
    }
    let results = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap()
            .is_some()
    );
}

#[test]
fn fixed_sequence_refuses_skips_wrong_targets_pending_overlap_and_early_close() {
    let fixture = Fixture::new();
    let operation = fixture.reserve();
    for step in [
        CommissioningStep::ApplyTarget(0),
        CommissioningStep::PrepareTarget(13),
        CommissioningStep::CompletionDurable,
    ] {
        assert!(
            fixture
                .store()
                .admit_commissioning_step(
                    operation.id,
                    &CommissioningStepRequest {
                        expected: operation.current.clone(),
                        step,
                        local_custody_sha256: "a".repeat(64)
                    },
                    &fixture.scope.operator,
                    Utc::now()
                )
                .is_err()
        );
    }
    let pending = fixture.admit(&operation);
    assert!(pending.next_step(Utc::now()).is_err());
    assert!(
        fixture
            .store()
            .close_commissioning_operation(
                operation.id,
                &pending.current,
                &fixture.scope.operator,
                b"invented terminal evidence",
                Utc::now()
            )
            .is_err()
    );
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(operation.id)
            .unwrap(),
        pending
    );
}

#[test]
fn complete_target_plan_closes_once_and_only_then_releases_pair() {
    let fixture = Fixture::new();
    let mut operation = fixture.reserve();
    while operation.next_step(Utc::now()).unwrap().is_some() {
        operation = fixture.complete(&fixture.admit(&operation));
        assert!(
            fixture
                .store()
                .commissioning_barrier(fixture.scope.backing_pair_id)
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(operation.steps.len(), 55);
    let connection = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fixture_refuse_barrier_close BEFORE UPDATE ON forge_commissioning_operations BEGIN SELECT RAISE(ABORT, 'injected closure persistence failure'); END;").unwrap();
    let expected = operation.current.clone();
    assert!(
        fixture
            .store()
            .close_commissioning_operation(
                operation.id,
                &expected,
                &fixture.scope.operator,
                b"complete actual fixture inventory",
                Utc::now()
            )
            .is_err()
    );
    // The preceding terminal record and barrier transition rolled back together.
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(operation.id)
            .unwrap(),
        operation
    );
    assert!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap()
            .is_some()
    );
    connection
        .execute_batch("DROP TRIGGER fixture_refuse_barrier_close;")
        .unwrap();
    operation = fixture
        .store()
        .close_commissioning_operation(
            operation.id,
            &operation.current,
            &fixture.scope.operator,
            b"complete actual fixture inventory",
            Utc::now(),
        )
        .unwrap();
    assert!(!operation.blocks_admission());
    assert_eq!(
        fixture
            .store()
            .close_commissioning_operation(
                operation.id,
                &expected,
                &fixture.scope.operator,
                b"complete actual fixture inventory",
                Utc::now()
            )
            .unwrap(),
        operation
    );
    assert!(
        fixture
            .store()
            .close_commissioning_operation(
                operation.id,
                &expected,
                &fixture.scope.operator,
                b"conflicting late response",
                Utc::now()
            )
            .is_err()
    );
    assert!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap()
            .is_none()
    );
    let mut successor = fixture.scope.clone();
    successor.request.idempotency_key = "after-verified-close".into();
    assert!(
        fixture
            .store()
            .reserve_commissioning_restore(&successor, Utc::now())
            .is_ok()
    );
}

#[test]
fn late_completion_retains_truth_without_progress_or_barrier_release() {
    let fixture = Fixture::new();
    let pending = fixture.admit(&fixture.reserve());
    let request = Fixture::completion(&pending);
    let late = fixture
        .store()
        .complete_commissioning_step(
            pending.id,
            &request,
            &fixture.scope.operator,
            CommissioningRecordingAuthority::CurrentOperator,
            fixture.scope.expires_at,
        )
        .unwrap();
    let completion = late.steps.last().unwrap().completion.as_ref().unwrap();
    assert_eq!(completion.outcome, CommissioningEffectOutcome::Verified);
    assert!(!completion.authorizes_progress);
    assert!(late.recovery_required && late.blocks_admission());
    assert!(
        late.next_step(fixture.scope.expires_at + Duration::seconds(1))
            .is_err()
    );
    assert!(
        fixture
            .store()
            .close_commissioning_operation(
                late.id,
                &late.current,
                &fixture.scope.operator,
                b"late",
                fixture.scope.expires_at
            )
            .is_err()
    );
    assert_eq!(
        fixture
            .store()
            .complete_commissioning_step(
                pending.id,
                &request,
                &fixture.scope.operator,
                CommissioningRecordingAuthority::RecoveryRecorder,
                fixture.scope.expires_at + Duration::seconds(20)
            )
            .unwrap(),
        late
    );
}

#[test]
fn reservation_step_and_completion_replay_keep_original_ids_and_conflicts_refuse() {
    let fixture = Fixture::new();
    let reserved = fixture.reserve();
    assert_eq!(fixture.reserve(), reserved);
    let request = CommissioningStepRequest {
        expected: reserved.current.clone(),
        step: CommissioningStep::CreateStage,
        local_custody_sha256: "b".repeat(64),
    };
    let pending = fixture
        .store()
        .admit_commissioning_step(reserved.id, &request, &fixture.scope.operator, Utc::now())
        .unwrap();
    assert_eq!(
        fixture
            .store()
            .admit_commissioning_step(reserved.id, &request, &fixture.scope.operator, Utc::now())
            .unwrap(),
        pending
    );
    let done = fixture.complete(&pending);
    assert_eq!(fixture.complete(&pending), done);
    let mut changed = Fixture::completion(&pending);
    changed.evidence.push(0);
    assert!(matches!(
        fixture.store().complete_commissioning_step(
            done.id,
            &changed,
            &fixture.scope.operator,
            CommissioningRecordingAuthority::CurrentOperator,
            Utc::now()
        ),
        Err(ForgeError::Conflict(_))
    ));
    let mut changed = fixture.scope.clone();
    changed.desired_present[0] = false;
    assert!(
        fixture
            .store()
            .reserve_commissioning_restore(&changed, Utc::now())
            .is_err()
    );
    assert_eq!(
        fixture.store().commissioning_operation(done.id).unwrap(),
        done
    );
}

#[test]
fn database_immutable_records_and_barrier_cannot_be_updated_or_deleted() {
    let fixture = Fixture::new();
    let operation = fixture.reserve();
    let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    for sql in [
        "UPDATE forge_commissioning_records SET record_json = '{}'",
        "DELETE FROM forge_commissioning_records",
        "UPDATE forge_commissioning_operations SET closed = 1",
        "DELETE FROM forge_commissioning_operations",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(operation.id)
            .unwrap(),
        operation
    );
    // Deliberately damaged isolated storage, after asserting production refusal.
    conn.execute_batch("DROP TRIGGER forge_commissioning_record_no_update;")
        .unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE forge_commissioning_records SET record_sha256 = ?1 WHERE operation_id = ?2",
            params!["0".repeat(64), operation.id.to_string()]
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        fixture.store().commissioning_operation(operation.id),
        Err(ForgeError::Storage(_))
    ));
}

#[test]
fn malformed_scope_and_backward_restoration_after_accepted_c_refuse() {
    let fixture = Fixture::new();
    let mut scope = fixture.scope.clone();
    scope.observed_main = scope.c.clone();
    assert!(scope.validate().is_err());
    scope.request.kind = CommissioningRestoreKind::RecoverAcceptedCForward;
    assert!(scope.validate().is_ok());
    scope.expires_at = scope.not_before + Duration::seconds(7201);
    assert!(scope.validate().is_err());
    scope.expires_at = scope.not_before + Duration::seconds(7200);
    assert!(scope.validate().is_ok());
    assert!(scope.require_window(scope.expires_at).is_err());
    assert!(
        scope
            .require_window(scope.expires_at - Duration::nanoseconds(1))
            .is_ok()
    );
}

#[test]
fn missing_production_authority_is_unavailable_with_actual_authenticated_actor() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.core.reserve_commissioning_operation(
            &fixture.operator,
            fixture.scope.repository_id,
            fixture.scope.contract_id,
            fixture.scope.request.clone()
        ),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn revoked_operator_cannot_record_but_separately_authenticated_recorder_preserves_outcome() {
    let mut fixture = Fixture::new();
    fixture.attach();
    fixture
        .core
        .revoke_personal_access_token("operator", fixture.operator_token)
        .unwrap();
    // Seed retained interrupted history directly in this isolated store after
    // revocation. Ordinary credential changes during an active barrier now
    // correctly refuse; they cannot be used to construct this recovery fixture.
    let pending = fixture.admit(&fixture.reserve());
    let address = fixture.address(&pending);
    let completion = Fixture::completion(&pending);
    assert!(
        fixture
            .core
            .complete_commissioning_step(&fixture.operator, address, completion.clone())
            .is_err()
    );
    let recorded = fixture
        .core
        .complete_commissioning_step(&fixture.recorder, address, completion)
        .unwrap();
    assert!(recorded.recovery_required && recorded.blocks_admission());
    assert!(
        !recorded
            .steps
            .last()
            .unwrap()
            .completion
            .as_ref()
            .unwrap()
            .authorizes_progress
    );
    assert!(
        fixture
            .core
            .admit_commissioning_step(
                &fixture.recorder,
                address,
                CommissioningStepRequest {
                    expected: recorded.current.clone(),
                    step: CommissioningStep::PrepareTarget(0),
                    local_custody_sha256: "a".repeat(64)
                }
            )
            .is_err()
    );
    assert_eq!(
        fixture
            .core
            .commissioning_operation(&fixture.recorder, address)
            .unwrap(),
        recorded
    );
    let mut foreign = address;
    foreign.contract_id = Uuid::new_v4();
    assert!(matches!(
        fixture
            .core
            .commissioning_operation(&fixture.recorder, foreign),
        Err(ForgeError::NotFound(_))
    ));
}

#[test]
fn replay_returns_original_prefix_but_never_hides_later_corruption() {
    let fixture = Fixture::new();
    let reserved = fixture.reserve();
    let pending = fixture.admit(&reserved);
    let completed = fixture.complete(&pending);
    let later = fixture.complete(&fixture.admit(&completed));
    assert_eq!(fixture.reserve(), reserved);
    assert_eq!(fixture.admit(&reserved), pending);
    assert_eq!(fixture.complete(&pending), completed);
    assert_eq!(
        fixture
            .store()
            .commissioning_operation(reserved.id)
            .unwrap(),
        later
    );
    let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    let sql = "UPDATE forge_commissioning_records SET record_sha256 = ?1 WHERE operation_id = ?2 AND revision = ?3";
    assert!(
        conn.execute(
            sql,
            params!["0".repeat(64), later.id.to_string(), later.current.revision]
        )
        .is_err()
    );
    conn.execute_batch("DROP TRIGGER forge_commissioning_record_no_update;")
        .unwrap();
    assert_eq!(
        conn.execute(
            sql,
            params!["0".repeat(64), later.id.to_string(), later.current.revision]
        )
        .unwrap(),
        1
    );
    assert!(
        fixture
            .store()
            .reserve_commissioning_restore(&fixture.scope, Utc::now())
            .is_err()
    );
    assert!(
        fixture
            .store()
            .admit_commissioning_step(
                reserved.id,
                &CommissioningStepRequest {
                    expected: reserved.current.clone(),
                    step: CommissioningStep::CreateStage,
                    local_custody_sha256: "a".repeat(64)
                },
                &fixture.scope.operator,
                Utc::now()
            )
            .is_err()
    );
    assert!(
        fixture
            .store()
            .complete_commissioning_step(
                reserved.id,
                &Fixture::completion(&pending),
                &fixture.scope.operator,
                CommissioningRecordingAuthority::CurrentOperator,
                Utc::now()
            )
            .is_err()
    );
}

#[test]
fn each_authority_callback_must_preserve_full_custody_before_durable_effect() {
    for selected in [1, 2, 3] {
        let mut fixture = Fixture::new();
        let mut operation = fixture.reserve();
        if selected == 2 {
            operation = fixture.admit(&operation);
        }
        if selected == 3 {
            while operation.next_step(Utc::now()).unwrap().is_some() {
                operation = fixture.complete(&fixture.admit(&operation));
            }
        }
        let authority = fixture.attach();
        authority.tamper_action.store(selected, Ordering::SeqCst);
        let address = fixture.address(&operation);
        let result = match selected {
            1 => fixture.core.admit_commissioning_step(
                &fixture.operator,
                address,
                CommissioningStepRequest {
                    expected: operation.current.clone(),
                    step: CommissioningStep::CreateStage,
                    local_custody_sha256: "a".repeat(64),
                },
            ),
            2 => fixture.core.complete_commissioning_step(
                &fixture.operator,
                address,
                Fixture::completion(&operation),
            ),
            3 => fixture.core.close_commissioning_operation(
                &fixture.operator,
                address,
                operation.current.clone(),
                b"complete actual fixture inventory".to_vec(),
            ),
            _ => unreachable!(),
        };
        assert!(
            matches!(result, Err(ForgeError::WriterUnavailable(_))),
            "action {selected}: {result:?}"
        );
        std::fs::set_permissions(
            &authority.custody.storage_root.path,
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert_eq!(
            fixture
                .store()
                .commissioning_operation(operation.id)
                .unwrap(),
            operation,
            "action {selected}"
        );
        assert_eq!(
            fixture
                .store()
                .commissioning_barrier(fixture.scope.backing_pair_id)
                .unwrap(),
            Some(operation)
        );
    }
}

#[test]
fn purportedly_closed_header_without_terminal_chain_never_removes_barrier() {
    let fixture = Fixture::new();
    let operation = fixture.reserve();
    let conn = Connection::open(fixture.directory.path().join("forge.sqlite")).unwrap();
    let sql = "UPDATE forge_commissioning_operations SET closed = 1 WHERE id = ?1";
    assert!(
        conn.execute(sql, params![operation.id.to_string()])
            .is_err()
    );
    assert_eq!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id)
            .unwrap(),
        Some(operation.clone())
    );
    conn.execute_batch("DROP TRIGGER forge_commissioning_operation_guard;")
        .unwrap();
    assert_eq!(
        conn.execute(sql, params![operation.id.to_string()])
            .unwrap(),
        1
    );
    assert!(matches!(
        fixture
            .store()
            .commissioning_barrier(fixture.scope.backing_pair_id),
        Err(ForgeError::Storage(_))
    ));
    let mut successor = fixture.scope.clone();
    successor.request.idempotency_key = "after-damaged-header".into();
    assert!(
        fixture
            .store()
            .reserve_commissioning_restore(&successor, Utc::now())
            .is_err()
    );
    successor.backing_pair_id = Uuid::new_v4();
    assert!(
        fixture
            .store()
            .reserve_commissioning_restore(&successor, Utc::now())
            .is_err()
    );
    let count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM forge_commissioning_operations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
