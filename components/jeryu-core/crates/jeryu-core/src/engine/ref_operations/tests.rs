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
