use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};

use chrono::TimeDelta;
use rusqlite::{Connection, params};
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::{CreatePullRequestRequest, CreateRepositoryRequest, PullRequestState};

const OLD: &str = "1111111111111111111111111111111111111111";
const NEW: &str = "2222222222222222222222222222222222222222";
const OTHER: &str = "3333333333333333333333333333333333333333";

fn fixture() -> (TempDir, PathBuf, ForgeCore, RefOperationIntent) {
    let directory = tempfile::Builder::new()
        .permissions(Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let database = directory.path().join("forge.sqlite");
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let repo = core
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
            "author",
            CreatePullRequestRequest {
                title: "persist the authorized closure".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(NEW.into()),
                ..Default::default()
            },
        )
        .unwrap();
    let intent = RefOperationIntent {
        repository_id: repo.id,
        idempotency_key: "merge-request".into(),
        actor: "merger".into(),
        operation: DurableRefOperationKind::Merge {
            pull_request_id: pull.id,
            source_repository_id: repo.id,
            source_ref: "refs/heads/topic".into(),
            source_head: NEW.into(),
        },
        changes: vec![RefChangeIntent {
            reference: "refs/heads/main".into(),
            expected: RefValue::Exact(OLD.into()),
            result: RefValue::Exact(NEW.into()),
        }],
        qualification_snapshot: json!({
            "policy_revision": "policy-17", "policy": {"required_contexts": ["demo/required"], "approvals": 1},
            "review_ids": [Uuid::new_v4()], "attempt_ids": [Uuid::new_v4()],
            "actor_authorization": {"login": "merger", "auth_epoch": 3, "grant": "merge"},
            "blockers": [], "source_evidence": {"head": NEW, "base": OLD, "tree": OTHER}
        }),
        expires_at: Utc::now() + TimeDelta::hours(1),
    };
    (directory, database, core, intent)
}

fn applied(operation: &DurableRefOperation) -> RefOperationObservation {
    RefOperationObservation::Observed {
        refs: operation
            .intent
            .changes
            .iter()
            .map(|change| ObservedRef {
                reference: change.reference.clone(),
                value: change.result.clone(),
            })
            .collect(),
        marker: RefValue::Exact(operation.marker_oid.clone()),
        additional_application_evidence: false,
    }
}

fn unchanged(operation: &DurableRefOperation) -> RefOperationObservation {
    RefOperationObservation::Observed {
        refs: operation
            .intent
            .changes
            .iter()
            .map(|change| ObservedRef {
                reference: change.reference.clone(),
                value: change.expected.clone(),
            })
            .collect(),
        marker: RefValue::Absent,
        additional_application_evidence: false,
    }
}

fn close_pull(state: &mut State) -> Result<()> {
    let pull = state
        .pulls
        .get_mut(&("owner".into(), "demo".into(), 1))
        .unwrap();
    pull.state = PullRequestState::Closed;
    pull.merged = true;
    pull.merged_at = Some(Utc::now());
    pull.merge_commit_sha = Some(NEW.into());
    Ok(())
}

fn counts(database: &PathBuf) -> (i64, i64, i64) {
    let conn = Connection::open(database).unwrap();
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM forge_ref_operations),
         (SELECT COUNT(*) FROM forge_audit_log WHERE action = 'repository.ref_operation'),
         (SELECT COUNT(*) FROM forge_ref_operation_outbox)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .unwrap()
}

#[test]
fn lost_prepare_return_and_snapshot_save_preserve_exact_restart_identity() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    core.set_repository_readme("owner", "demo", "unrelated snapshot".into())
        .unwrap();
    core.ensure_user("another-profile").unwrap();
    assert_eq!(counts(&database), (1, 1, 0));
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        core.prepare_ref_operation(intent.clone()).unwrap(),
        prepared
    );
    assert_eq!(
        core.get_ref_operation_by_key(intent.repository_id, &intent.idempotency_key)
            .unwrap(),
        Some(prepared.clone())
    );
    assert_eq!(
        prepared.qualification_sha256,
        sha256(&canonical_json(&intent.qualification_snapshot).unwrap())
    );
    assert_eq!(
        prepared.intent.qualification_snapshot["policy_revision"],
        "policy-17"
    );
    assert_eq!(counts(&database), (1, 1, 0));
}

#[test]
fn concurrent_identical_prepare_returns_one_operation_and_changed_input_conflicts() {
    let (_directory, database, core, intent) = fixture();
    let barrier = Arc::new(Barrier::new(5));
    let workers = (0..4)
        .map(|_| {
            let core = core.clone();
            let request = intent.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                core.prepare_ref_operation(request).unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let operations = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert!(
        operations
            .iter()
            .all(|operation| operation == &operations[0])
    );
    for change in ["actor", "policy", "attempt", "ref"] {
        let mut changed = intent.clone();
        match change {
            "actor" => changed.actor = "another-merger".into(),
            "policy" => changed.qualification_snapshot["policy_revision"] = json!("policy-18"),
            "attempt" => changed.qualification_snapshot["attempt_ids"] = json!([Uuid::new_v4()]),
            "ref" => changed.changes[0].expected = RefValue::Exact(OTHER.into()),
            _ => unreachable!(),
        }
        assert!(
            matches!(
                core.prepare_ref_operation(changed),
                Err(ForgeError::Conflict(_))
            ),
            "{change}"
        );
    }
    assert_eq!(counts(&database), (1, 1, 0));
}

#[test]
fn committed_state_audit_and_event_survive_restart_and_lost_return_without_reapplying() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let observation = applied(&prepared);
    let committed = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            observation.clone(),
            close_pull,
        )
        .unwrap();
    assert_eq!(committed.state, DurableRefOperationState::Committed);
    assert!(core.get_pull_request("owner", "demo", 1).unwrap().merged);
    let events = core
        .pending_ref_operation_events(intent.repository_id, 10)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operation, committed);
    assert_eq!(counts(&database), (1, 2, 1));
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let replay = core
        .reconcile_ref_operation(intent.repository_id, prepared.id, observation, |_| {
            panic!("lost return must not repeat closure")
        })
        .unwrap();
    assert_eq!(replay, committed);
    assert_eq!(
        core.prepare_ref_operation(intent.clone()).unwrap(),
        committed
    );
    assert_eq!(
        core.pending_ref_operation_events(intent.repository_id, 10)
            .unwrap(),
        events
    );
    assert!(core.get_pull_request("owner", "demo", 1).unwrap().merged);
    assert_eq!(counts(&database), (1, 2, 1));
}

#[test]
fn faults_roll_back_proposed_pull_closure_operation_audit_and_outbox_together() {
    for trigger in [
        "CREATE TRIGGER injected_failure BEFORE UPDATE ON pull_requests BEGIN SELECT RAISE(ABORT, 'state fault'); END;",
        "CREATE TRIGGER injected_failure BEFORE UPDATE ON forge_ref_operations BEGIN SELECT RAISE(ABORT, 'operation fault'); END;",
        "CREATE TRIGGER injected_failure BEFORE INSERT ON forge_audit_log WHEN NEW.phase = 'completed' BEGIN SELECT RAISE(ABORT, 'audit fault'); END;",
        "CREATE TRIGGER injected_failure BEFORE INSERT ON forge_ref_operation_outbox BEGIN SELECT RAISE(ABORT, 'outbox fault'); END;",
    ] {
        let (_directory, database, core, intent) = fixture();
        let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
        let before = core.get_pull_request("owner", "demo", 1).unwrap();
        let conn = Connection::open(&database).unwrap();
        conn.execute_batch(trigger).unwrap();
        let result = core.reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        );
        assert!(
            matches!(result, Err(ForgeError::Storage(_))),
            "{trigger}: {result:?}"
        );
        assert_eq!(core.get_pull_request("owner", "demo", 1).unwrap(), before);
        assert_eq!(
            core.get_ref_operation(intent.repository_id, prepared.id)
                .unwrap(),
            prepared
        );
        assert_eq!(counts(&database), (1, 1, 0));
        drop(core);
        let core = ForgeCore::open_sqlite(&database).unwrap();
        assert_eq!(core.get_pull_request("owner", "demo", 1).unwrap(), before);
        assert_eq!(
            core.get_ref_operation(intent.repository_id, prepared.id)
                .unwrap(),
            prepared
        );
        conn.execute_batch("DROP TRIGGER injected_failure").unwrap();
        core.reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        )
        .unwrap();
        assert!(core.get_pull_request("owner", "demo", 1).unwrap().merged);
        assert_eq!(counts(&database), (1, 2, 1));
    }
}

#[test]
fn state_closure_error_does_not_publish_partial_memory_or_sqlite_state() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let before = core.get_pull_request("owner", "demo", 1).unwrap();
    let result = core.reconcile_ref_operation(
        intent.repository_id,
        prepared.id,
        applied(&prepared),
        |state| {
            close_pull(state)?;
            Err(ForgeError::Validation("closure refused".into()))
        },
    );
    assert!(matches!(result, Err(ForgeError::Validation(_))));
    assert_eq!(core.get_pull_request("owner", "demo", 1).unwrap(), before);
    assert_eq!(counts(&database), (1, 1, 0));
}

#[test]
fn exact_nonapplication_aborts_without_closure_or_event_and_requires_a_new_attempt() {
    let (_directory, database, core, mut intent) = fixture();
    intent.operation = DurableRefOperationKind::RefUpdate;
    intent.changes = vec![
        RefChangeIntent {
            reference: "refs/heads/new".into(),
            expected: RefValue::Absent,
            result: RefValue::Exact(NEW.into()),
        },
        RefChangeIntent {
            reference: "refs/tags/retired".into(),
            expected: RefValue::Exact(OLD.into()),
            result: RefValue::Absent,
        },
    ];
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let observation = unchanged(&prepared);
    let aborted = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            observation.clone(),
            |_| panic!("not applied"),
        )
        .unwrap();
    assert_eq!(aborted.state, DurableRefOperationState::AbortedNotApplied);
    assert_eq!(
        core.reconcile_ref_operation(intent.repository_id, prepared.id, observation, |_| panic!(
            "replay"
        ))
        .unwrap(),
        aborted
    );
    assert!(matches!(
        core.reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull
        ),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(counts(&database), (1, 2, 0));
    intent.idempotency_key = "fresh-qualified-attempt".into();
    assert_ne!(core.prepare_ref_operation(intent).unwrap().id, prepared.id);
}

#[test]
fn ambiguous_observations_quarantine_and_ordinary_retries_cannot_clear_them() {
    for case in [
        "missing-marker",
        "wrong-marker",
        "wrong-ref",
        "missing-ref",
        "duplicate-ref",
        "extra-evidence",
        "unavailable",
    ] {
        let (_directory, database, core, mut intent) = fixture();
        let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
        let mut observation = if case == "extra-evidence" {
            unchanged(&prepared)
        } else {
            applied(&prepared)
        };
        if let RefOperationObservation::Observed {
            refs,
            marker,
            additional_application_evidence,
        } = &mut observation
        {
            match case {
                "missing-marker" => *marker = RefValue::Absent,
                "wrong-marker" => *marker = RefValue::Exact(OTHER.into()),
                "wrong-ref" => refs[0].value = RefValue::Exact(OTHER.into()),
                "missing-ref" => refs.clear(),
                "duplicate-ref" => refs.push(refs[0].clone()),
                "extra-evidence" => *additional_application_evidence = true,
                "unavailable" => {}
                _ => unreachable!(),
            }
        }
        if case == "unavailable" {
            observation = RefOperationObservation::Unavailable {
                reason: "Git read interrupted".into(),
            };
        }
        let isolated = core
            .reconcile_ref_operation(
                intent.repository_id,
                prepared.id,
                observation.clone(),
                |_| panic!("ambiguous evidence"),
            )
            .unwrap();
        assert_eq!(
            isolated.state,
            DurableRefOperationState::ReconciliationRequired,
            "{case}"
        );
        drop(core);
        let core = ForgeCore::open_sqlite(&database).unwrap();
        assert_eq!(
            core.reconcile_ref_operation(
                intent.repository_id,
                prepared.id,
                observation,
                |_| panic!("retry")
            )
            .unwrap(),
            isolated
        );
        assert!(matches!(
            core.reconcile_ref_operation(
                intent.repository_id,
                prepared.id,
                applied(&prepared),
                close_pull
            ),
            Err(ForgeError::Conflict(_))
        ));
        intent.idempotency_key = "ordinary-retry".into();
        assert!(matches!(
            core.prepare_ref_operation(intent),
            Err(ForgeError::WriterUnavailable(_))
        ));
        assert_eq!(counts(&database), (1, 2, 0));
    }
}

#[test]
fn outbox_delivery_is_scoped_and_idempotent_after_restart() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let receipt = "a".repeat(64);
    assert!(matches!(
        core.acknowledge_ref_operation_event(intent.repository_id, prepared.id, &receipt),
        Err(ForgeError::NotFound(_))
    ));
    core.reconcile_ref_operation(
        intent.repository_id,
        prepared.id,
        applied(&prepared),
        close_pull,
    )
    .unwrap();
    let event = core
        .pending_ref_operation_events(intent.repository_id, 10)
        .unwrap()
        .remove(0);
    assert!(matches!(
        core.acknowledge_ref_operation_event(Uuid::new_v4(), event.id, &receipt),
        Err(ForgeError::NotFound(_))
    ));
    let delivered = core
        .acknowledge_ref_operation_event(intent.repository_id, event.id, &receipt)
        .unwrap();
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        core.acknowledge_ref_operation_event(intent.repository_id, event.id, &receipt)
            .unwrap(),
        delivered
    );
    assert!(matches!(
        core.acknowledge_ref_operation_event(intent.repository_id, event.id, &"b".repeat(64)),
        Err(ForgeError::Conflict(_))
    ));
    assert!(
        core.pending_ref_operation_events(intent.repository_id, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(counts(&database), (1, 2, 1));
}

#[test]
fn catalog_deletion_does_not_cascade_recovery_audit_or_pending_event() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let committed = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        )
        .unwrap();
    let events = core
        .pending_ref_operation_events(intent.repository_id, 10)
        .unwrap();
    core.delete_repository("owner", "demo").unwrap();
    core.ensure_user("unrelated-after-delete").unwrap();
    assert_eq!(counts(&database), (1, 2, 1));
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        core.get_ref_operation(intent.repository_id, prepared.id)
            .unwrap(),
        committed
    );
    assert_eq!(
        core.prepare_ref_operation(intent.clone()).unwrap(),
        committed
    );
    assert_eq!(
        core.pending_ref_operation_events(intent.repository_id, 10)
            .unwrap(),
        events
    );
    let audit = core
        .list_audit(&format!("repository:{}", intent.repository_id))
        .unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].detail["operation"]["id"], prepared.id.to_string());
}

#[test]
fn repository_uuid_scopes_the_key_and_readback_without_slug_inference() {
    let (_directory, _database, core, intent) = fixture();
    let first = core.prepare_ref_operation(intent.clone()).unwrap();
    let other = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "other".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let mut other_intent = intent.clone();
    other_intent.repository_id = other.id;
    other_intent.operation = DurableRefOperationKind::RefUpdate;
    let second = core.prepare_ref_operation(other_intent).unwrap();
    assert_ne!(first.id, second.id);
    assert!(matches!(
        core.get_ref_operation(other.id, first.id),
        Err(ForgeError::NotFound(_))
    ));
    assert_eq!(
        core.get_ref_operation_by_key(other.id, &intent.idempotency_key)
            .unwrap(),
        Some(second)
    );
}

#[test]
fn unsupported_refs_oids_duplicate_refs_and_omitted_expectations_are_rejected() {
    let (_directory, database, core, intent) = fixture();
    for reference in [
        "main",
        "refs/jeryu/operations/owned",
        "refs/heads/",
        "refs/heads/a..b",
        "refs/heads/.hidden",
        "refs/tags/a.lock",
        "refs/heads/a b",
    ] {
        let mut changed = intent.clone();
        changed.changes[0].reference = reference.into();
        assert!(
            matches!(
                core.prepare_ref_operation(changed),
                Err(ForgeError::Validation(_))
            ),
            "{reference}"
        );
    }
    for oid in [
        "HEAD",
        "1234567",
        "0000000000000000000000000000000000000000",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "1ggggggggggggggggggggggggggggggggggggggg",
    ] {
        let mut changed = intent.clone();
        changed.changes[0].expected = RefValue::Exact(oid.into());
        assert!(matches!(
            core.prepare_ref_operation(changed),
            Err(ForgeError::Validation(_))
        ));
    }
    let mut duplicate = intent.clone();
    duplicate.changes.push(duplicate.changes[0].clone());
    assert!(matches!(
        core.prepare_ref_operation(duplicate),
        Err(ForgeError::Validation(_))
    ));
    let mut missing = serde_json::to_value(&intent).unwrap();
    missing["changes"][0]
        .as_object_mut()
        .unwrap()
        .remove("expected");
    assert!(serde_json::from_value::<RefOperationIntent>(missing).is_err());
    let mut digest_only = intent.clone();
    digest_only.qualification_snapshot = json!({"digest": "a".repeat(64)});
    assert!(matches!(
        core.prepare_ref_operation(digest_only),
        Err(ForgeError::Validation(_))
    ));
    assert_eq!(counts(&database), (0, 0, 0));
}

#[test]
fn sqlite_constraints_reject_terminal_rewrites_and_events_for_prepared_intents() {
    let (_directory, database, core, intent) = fixture();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let conn = Connection::open(&database).unwrap();
    assert!(
        conn.execute(
            "UPDATE forge_ref_operations SET intent_sha256 = ?1 WHERE id = ?2",
            params!["b".repeat(64), prepared.id.to_string()]
        )
        .is_err()
    );
    assert!(conn.execute("INSERT INTO forge_ref_operation_outbox (id, operation_id, repo_id, created_at, payload_json) VALUES (?1, ?2, ?3, 'now', '{}')",
        params![Uuid::new_v4().to_string(), prepared.id.to_string(), intent.repository_id.to_string()]).is_err());
    core.reconcile_ref_operation(
        intent.repository_id,
        prepared.id,
        unchanged(&prepared),
        |_| panic!("aborted"),
    )
    .unwrap();
    assert!(
        conn.execute(
            "UPDATE forge_ref_operations SET state = 'prepared', outcome_json = NULL WHERE id = ?1",
            [prepared.id.to_string()]
        )
        .is_err()
    );
    assert_eq!(counts(&database), (1, 2, 0));
}

#[test]
fn forward_migration_preserves_legacy_state_without_inventing_operations() {
    let (_directory, database, core, _intent) = fixture();
    let before = core.get_pull_request("owner", "demo", 1).unwrap();
    drop(core);
    Connection::open(&database)
        .unwrap()
        .execute_batch("DROP TABLE forge_ref_operation_outbox; DROP TABLE forge_ref_operations;")
        .unwrap();
    for _ in 0..2 {
        let reopened = ForgeCore::open_sqlite(&database).unwrap();
        assert_eq!(
            reopened.get_pull_request("owner", "demo", 1).unwrap(),
            before
        );
        assert_eq!(counts(&database), (0, 0, 0));
    }
}

#[test]
fn memory_only_core_never_returns_a_fictitious_durable_receipt() {
    let (_directory, _database, _core, intent) = fixture();
    assert!(matches!(
        ForgeCore::new().prepare_ref_operation(intent),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn expired_prepared_authorization_does_not_undo_a_proven_applied_operation() {
    let (_directory, database, core, mut intent) = fixture();
    let prepared_at = Utc::now() - TimeDelta::hours(2);
    intent.expires_at = prepared_at + TimeDelta::hours(1);
    assert!(matches!(
        core.prepare_ref_operation(intent.clone()),
        Err(ForgeError::Validation(_))
    ));
    // Controlled historical server time, through the same private store method;
    // no sleep, raw binding rewrite or current-eligibility exception is needed.
    let prepared = core
        .operation_storage()
        .unwrap()
        .prepare_ref_operation(&intent, prepared_at)
        .unwrap();
    assert_eq!(
        core.prepare_ref_operation(intent.clone()).unwrap(),
        prepared
    );
    core.block_repository_mutations(
        intent.repository_id,
        crate::RepositoryMutationBlock::ReadOnly {
            reason: "custody changed after recorded application".into(),
            evidence: "fixture custody receipt".into(),
        },
    )
    .unwrap();
    let committed = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        )
        .unwrap();
    assert_eq!(committed.state, DurableRefOperationState::Committed);
    assert!(
        core.repository_mutation_block(intent.repository_id)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        core.prepare_ref_operation(intent.clone()).unwrap(),
        committed
    );
    drop(core);
    let reopened = ForgeCore::open_sqlite(&database).unwrap();
    assert_eq!(
        reopened
            .get_ref_operation(intent.repository_id, prepared.id)
            .unwrap(),
        committed
    );
    assert!(
        reopened
            .get_pull_request("owner", "demo", 1)
            .unwrap()
            .merged
    );
}

#[test]
fn qualification_object_key_order_does_not_change_the_persisted_binding() {
    let (_directory, database, core, mut intent) = fixture();
    intent.qualification_snapshot = serde_json::from_str(r#"{
      "policy_revision":"r1", "policy":{"z":1,"a":{"y":2,"b":3}},
      "review_ids":[], "attempt_ids":[], "actor_authorization":{"z":3,"login":"merger"}, "blockers":[]
    }"#).unwrap();
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    intent.qualification_snapshot = serde_json::from_str(r#"{
      "blockers":[], "actor_authorization":{"login":"merger","z":3}, "attempt_ids":[], "review_ids":[],
      "policy":{"a":{"b":3,"y":2},"z":1}, "policy_revision":"r1"
    }"#).unwrap();
    assert_eq!(core.prepare_ref_operation(intent).unwrap(), prepared);
    drop(core);
    assert_eq!(
        ForgeCore::open_sqlite(&database)
            .unwrap()
            .get_ref_operation(prepared.intent.repository_id, prepared.id)
            .unwrap(),
        prepared
    );
    assert_eq!(counts(&database), (1, 1, 0));
}

const READBACK_PASSWORD: &str = "readback fixture strong password 5130490501483594442";

fn readback_actor(
    core: &ForgeCore,
    login: &str,
    role: UserRole,
) -> (AuthenticatedActor, Uuid, String) {
    core.create_account(login, READBACK_PASSWORD, role).unwrap();
    let session = core.create_session(login, READBACK_PASSWORD).unwrap();
    let actor = core
        .authenticate_actor(crate::ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let pat = core
        .create_personal_access_token(&actor, "readback fixture", None)
        .unwrap();
    (
        core.authenticate_actor(crate::ActorCredential::PersonalAccessToken(&pat.secret))
            .unwrap(),
        pat.token.id,
        pat.secret,
    )
}

#[test]
fn merge_readback_revalidates_uuid_grant_and_runtime_before_disclosure() {
    let (_dir, _db, core, intent) = fixture();
    core.set_repository_visibility("owner", "demo", true)
        .unwrap();
    let (actor, _, _) = readback_actor(&core, "reader", UserRole::User);
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    assert!(matches!(
        core.merge_operation(&actor, intent.repository_id, prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
    core.grant_repo_access(
        "fixture-operator",
        "reader",
        "owner",
        "demo",
        crate::RepoAccessLevel::Read,
    )
    .unwrap();
    assert_eq!(
        core.merge_operation(&actor, intent.repository_id, prepared.id)
            .unwrap()
            .operation,
        prepared
    );
    assert!(matches!(
        core.merge_operation(&actor, Uuid::new_v4(), prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        core.merge_operation(&actor, Uuid::nil(), prepared.id),
        Err(ForgeError::Validation(_))
    ));
    let (_other_dir, _other_db, other, _) = fixture();
    assert!(matches!(
        other.merge_operation(&actor, intent.repository_id, prepared.id),
        Err(ForgeError::Unauthenticated(_))
    ));
    core.revoke_repo_access("reader", "owner", "demo").unwrap();
    assert!(matches!(
        core.merge_operation(&actor, intent.repository_id, prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
}

#[test]
fn merge_readback_accepts_read_only_sessions_but_refuses_revoked_and_expired_credentials() {
    let (_dir, _db, core, intent) = fixture();
    let (actor, token_id, _) = readback_actor(&core, "reader", UserRole::User);
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let session = core.create_session("reader", READBACK_PASSWORD).unwrap();
    let readonly = core
        .authenticate_actor(crate::ActorCredential::SessionReadOnly(&session.token))
        .unwrap();
    assert_eq!(
        core.merge_operation(&readonly, intent.repository_id, prepared.id)
            .unwrap()
            .operation,
        prepared
    );
    core.revoke_personal_access_token("reader", token_id)
        .unwrap();
    assert!(matches!(
        core.merge_operation(&actor, intent.repository_id, prepared.id),
        Err(ForgeError::Unauthenticated(_))
    ));
    core.with_global_mutation(|| {
        let mut state = core.runtime.state.write();
        let previous = state.clone();
        let retained = state
            .sessions
            .values_mut()
            .find(|entry| entry.id == session.session.id)
            .unwrap();
        retained.expires_at = Utc::now() - TimeDelta::seconds(1);
        core.persist_after_mutation(&mut state, previous)
    })
    .unwrap();
    assert!(matches!(
        core.merge_operation(&readonly, intent.repository_id, prepared.id),
        Err(ForgeError::Unauthenticated(_))
    ));
}

#[test]
fn merge_readback_preserves_operation_and_delivery_across_acknowledgement_and_restart() {
    let (_dir, database, core, intent) = fixture();
    let (actor, _, secret) = readback_actor(&core, "reader", UserRole::User);
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    assert!(
        core.merge_operation(&actor, intent.repository_id, prepared.id)
            .unwrap()
            .delivery
            .is_none()
    );
    let committed = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        )
        .unwrap();
    let readback = core
        .merge_operation(&actor, intent.repository_id, prepared.id)
        .unwrap();
    assert_eq!(readback.operation, committed);
    let delivery = readback.delivery.unwrap();
    assert!(delivery.delivered_at.is_none());
    let receiving_hash = "a".repeat(64);
    let acknowledged = core
        .acknowledge_ref_operation_event(intent.repository_id, delivery.id, &receiving_hash)
        .unwrap();
    assert_eq!(
        core.merge_operation(&actor, intent.repository_id, prepared.id)
            .unwrap()
            .delivery,
        Some(acknowledged.clone())
    );
    core.ensure_user("unrelated-save-after-readback").unwrap();
    drop(actor);
    drop(core);
    let core = ForgeCore::open_sqlite(&database).unwrap();
    let actor = core
        .authenticate_actor(crate::ActorCredential::PersonalAccessToken(&secret))
        .unwrap();
    let reopened = core
        .merge_operation(&actor, intent.repository_id, prepared.id)
        .unwrap();
    assert_eq!(reopened.operation, committed);
    assert_eq!(reopened.delivery, Some(acknowledged));
}

#[test]
fn merge_readback_remains_available_when_operation_requires_recovery() {
    let (_dir, _db, core, intent) = fixture();
    let (actor, _, _) = readback_actor(&core, "reader", UserRole::User);
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let quarantined = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            RefOperationObservation::Unavailable {
                reason: "fixture lost backend response".into(),
            },
            |_| panic!("ambiguous observation cannot publish state"),
        )
        .unwrap();
    let readback = core
        .merge_operation(&actor, intent.repository_id, prepared.id)
        .unwrap();
    assert_eq!(readback.operation, quarantined);
    assert_eq!(
        readback.operation.state,
        DurableRefOperationState::ReconciliationRequired
    );
    assert!(readback.delivery.is_none());
    let mut later = intent;
    later.idempotency_key = "later-operation".into();
    assert!(matches!(
        core.prepare_ref_operation(later),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn merge_readback_deleted_uuid_requires_admin_and_slug_reuse_grants_nothing() {
    let (_dir, _db, core, intent) = fixture();
    let (reader, _, _) = readback_actor(&core, "reader", UserRole::User);
    let (admin, _, _) = readback_actor(&core, "administrator", UserRole::Admin);
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    let committed = core
        .reconcile_ref_operation(
            intent.repository_id,
            prepared.id,
            applied(&prepared),
            close_pull,
        )
        .unwrap();
    core.delete_repository("owner", "demo").unwrap();
    let successor = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "demo".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(successor.id, intent.repository_id);
    core.grant_repo_access(
        "fixture-operator",
        "reader",
        "owner",
        "demo",
        crate::RepoAccessLevel::Admin,
    )
    .unwrap();
    assert!(matches!(
        core.merge_operation(&reader, intent.repository_id, prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        core.merge_operation(&reader, successor.id, prepared.id),
        Err(ForgeError::NotFound(_))
    ));
    assert_eq!(
        core.merge_operation(&admin, intent.repository_id, prepared.id)
            .unwrap()
            .operation,
        committed
    );
}

#[test]
fn merge_readback_refuses_other_operation_kinds_and_corrupt_delivery() {
    let (_dir, _db, core, mut intent) = fixture();
    let (actor, _, _) = readback_actor(&core, "reader", UserRole::User);
    intent.operation = DurableRefOperationKind::RefUpdate;
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    assert!(matches!(
        core.merge_operation(&actor, intent.repository_id, prepared.id),
        Err(ForgeError::Validation(_))
    ));
    let (_dir2, database2, core2, intent2) = fixture();
    let (actor2, _, _) = readback_actor(&core2, "reader", UserRole::User);
    let prepared2 = core2.prepare_ref_operation(intent2.clone()).unwrap();
    core2
        .reconcile_ref_operation(
            intent2.repository_id,
            prepared2.id,
            applied(&prepared2),
            close_pull,
        )
        .unwrap();
    let conn = Connection::open(&database2).unwrap();
    let before = core2
        .merge_operation(&actor2, intent2.repository_id, prepared2.id)
        .unwrap();
    let denied = conn
        .execute(
            "UPDATE forge_ref_operation_outbox SET payload_json = ?1 WHERE operation_id = ?2",
            params!["{}", prepared2.id.to_string()],
        )
        .unwrap_err();
    assert!(
        matches!(denied, rusqlite::Error::SqliteFailure(error, Some(message))
        if error.extended_code == 1811 && message == "immutable outbox event or delivery")
    );
    let after_denial = core2
        .merge_operation(&actor2, intent2.repository_id, prepared2.id)
        .unwrap();
    // observed_at belongs to this fresh read, not to the immutable stored result.
    assert_eq!(after_denial.operation, before.operation);
    assert_eq!(after_denial.delivery, before.delivery);
    // Simulate externally damaged storage only in this disposable fixture database.
    // The ordinary write above must remain denied by the production migration.
    conn.execute_batch("DROP TRIGGER forge_ref_operation_outbox_immutable")
        .unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE forge_ref_operation_outbox SET payload_json = ?1 WHERE operation_id = ?2",
            params!["{}", prepared2.id.to_string()],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        core2.merge_operation(&actor2, intent2.repository_id, prepared2.id),
        Err(ForgeError::Storage(_))
    ));
}

#[test]
fn merge_readback_requires_current_private_source_uuid_access_too() {
    let (_dir, _db, core, mut intent) = fixture();
    let (reader, _, _) = readback_actor(&core, "reader", UserRole::User);
    let source = core
        .create_repository(
            "fork-owner",
            CreateRepositoryRequest {
                name: "private-topic".into(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    if let DurableRefOperationKind::Merge {
        source_repository_id,
        ..
    } = &mut intent.operation
    {
        *source_repository_id = source.id;
    }
    let prepared = core.prepare_ref_operation(intent.clone()).unwrap();
    assert!(matches!(
        core.merge_operation(&reader, intent.repository_id, prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
    core.grant_repo_access(
        "fixture-operator",
        "reader",
        "fork-owner",
        "private-topic",
        crate::RepoAccessLevel::Read,
    )
    .unwrap();
    assert_eq!(
        core.merge_operation(&reader, intent.repository_id, prepared.id)
            .unwrap()
            .operation,
        prepared
    );
    core.revoke_repo_access("reader", "fork-owner", "private-topic")
        .unwrap();
    assert!(matches!(
        core.merge_operation(&reader, intent.repository_id, prepared.id),
        Err(ForgeError::Forbidden(_))
    ));
}

#[test]
fn merge_readback_refuses_a_valid_delivery_from_another_committed_operation() {
    let (_dir, database, core, intent) = fixture();
    let (reader, _, _) = readback_actor(&core, "reader", UserRole::User);
    let first = core.prepare_ref_operation(intent.clone()).unwrap();
    let first = core
        .reconcile_ref_operation(intent.repository_id, first.id, applied(&first), close_pull)
        .unwrap();
    let second_pull = core
        .create_pull_request(
            "owner",
            "demo",
            "author",
            CreatePullRequestRequest {
                title: "second journal fixture".into(),
                head: "second-topic".into(),
                base: "main".into(),
                head_sha: Some(OTHER.into()),
                base_sha: Some(NEW.into()),
                ..Default::default()
            },
        )
        .unwrap();
    let mut second_intent = intent.clone();
    second_intent.idempotency_key = "second-committed-operation".into();
    second_intent.operation = DurableRefOperationKind::Merge {
        pull_request_id: second_pull.id,
        source_repository_id: intent.repository_id,
        source_ref: "refs/heads/second-topic".into(),
        source_head: OTHER.into(),
    };
    second_intent.changes[0].expected = RefValue::Exact(NEW.into());
    second_intent.changes[0].result = RefValue::Exact(OTHER.into());
    second_intent.qualification_snapshot["source_evidence"]["head"] = json!(OTHER);
    second_intent.qualification_snapshot["source_evidence"]["base"] = json!(NEW);
    let second = core.prepare_ref_operation(second_intent).unwrap();
    let second = core
        .reconcile_ref_operation(intent.repository_id, second.id, applied(&second), |state| {
            let pull = state
                .pulls
                .get_mut(&("owner".into(), "demo".into(), second_pull.number))
                .unwrap();
            pull.merged = true;
            pull.state = PullRequestState::Merged;
            Ok(())
        })
        .unwrap();
    let second_delivery = core
        .merge_operation(&reader, intent.repository_id, second.id)
        .unwrap()
        .delivery
        .unwrap();
    assert_eq!(second_delivery.operation, second);
    let mut first_outcome = first.outcome.clone().unwrap();
    first_outcome.event_id = Some(second_delivery.id);
    let conn = Connection::open(&database).unwrap();
    let before = core
        .merge_operation(&reader, intent.repository_id, first.id)
        .unwrap();
    let denied = conn
        .execute(
            "UPDATE forge_ref_operations SET outcome_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&first_outcome).unwrap(),
                first.id.to_string()
            ],
        )
        .unwrap_err();
    assert!(
        matches!(denied, rusqlite::Error::SqliteFailure(error, Some(message))
        if error.extended_code == 1811 && message == "immutable ref operation binding or outcome")
    );
    let after_denial = core
        .merge_operation(&reader, intent.repository_id, first.id)
        .unwrap();
    assert_eq!(after_denial.operation, before.operation);
    assert_eq!(after_denial.delivery, before.delivery);
    // Simulate a damaged durable link only after proving the live trigger refuses it.
    conn.execute_batch("DROP TRIGGER forge_ref_operation_immutable")
        .unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE forge_ref_operations SET outcome_json = ?1 WHERE id = ?2",
            params![
                serde_json::to_string(&first_outcome).unwrap(),
                first.id.to_string()
            ],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        core.merge_operation(&reader, intent.repository_id, first.id),
        Err(ForgeError::Storage(_))
    ));
    assert_eq!(
        core.merge_operation(&reader, intent.repository_id, second.id)
            .unwrap()
            .delivery
            .unwrap(),
        second_delivery
    );
}
