use chrono::Duration;
use rusqlite::{Connection, params};
use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::core::storage::SqliteStore;
use crate::{ForgeCore, ReviewCredentialKind};

struct Fixture {
    directory: tempfile::TempDir,
    core: ForgeCore,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let core = ForgeCore::open_sqlite(directory.path().join("forge.sqlite")).unwrap();
        Self { directory, core }
    }

    fn store(&self) -> &SqliteStore {
        self.core.runtime.storage.as_ref().unwrap()
    }

    fn connection(&self) -> Connection {
        Connection::open(self.directory.path().join("forge.sqlite")).unwrap()
    }
}

fn reservation() -> RequiredAttemptReservation {
    RequiredAttemptReservation {
        binding: RequiredAttemptBinding {
            repository_id: Uuid::new_v4(),
            commit_sha: "a".repeat(40),
            tree_sha: "b".repeat(40),
            context: "example/required".into(),
            publisher_id: Uuid::new_v4(),
            actor: ReviewActorBinding {
                login: "publisher".into(),
                profile_id: Uuid::new_v4(),
                account_created_at: Utc::now() - Duration::days(1),
                auth_epoch: 7,
                credential_kind: ReviewCredentialKind::PersonalAccessToken,
                credential_id: Uuid::new_v4(),
            },
            runtime_sha256: "c".repeat(64),
            authority_origin: RequiredAuthorityOrigin::ReviewedStagingCandidate,
            enrollment_sha256: "d".repeat(64),
            evidence_contract_sha256: "e".repeat(64),
        },
        idempotency_key: Uuid::new_v4().to_string(),
        expires_at: Utc::now() + Duration::minutes(30),
    }
}

fn artifacts() -> Vec<RequiredArtifactBytes> {
    vec![
        RequiredArtifactBytes {
            name: "receipt.json".into(),
            bytes: br#"{"status":"executed"}"#.to_vec(),
        },
        RequiredArtifactBytes {
            name: "test.log".into(),
            bytes: b"actual captured fixture bytes\n".to_vec(),
        },
    ]
}

fn latest(f: &Fixture, request: &RequiredAttemptReservation) -> DurableRequiredAttempt {
    f.store()
        .latest_required_attempt(
            request.binding.repository_id,
            &request.binding.commit_sha,
            &request.binding.context,
        )
        .unwrap()
        .unwrap()
}

#[test]
fn newest_reservation_governs_before_its_completion() {
    let f = Fixture::new();
    let mut request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    let success = f
        .store()
        .complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        )
        .unwrap();
    assert_eq!(latest(&f, &request), success);
    assert_eq!(success.status_at(now), RequiredAttemptStatus::Success);
    request.idempotency_key = Uuid::new_v4().to_string();
    let newest = f.store().reserve_required_attempt(&request, now).unwrap();
    assert_eq!(newest.ordinal, first.ordinal + 1);
    assert_eq!(
        latest(&f, &request).status_at(now),
        RequiredAttemptStatus::Pending
    );
    assert_eq!(
        latest(&f, &request).status_at(request.expires_at),
        RequiredAttemptStatus::Expired
    );
    assert!(matches!(
        f.store().complete_required_attempt(
            newest.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            request.expires_at
        ),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(
        latest(&f, &request).status_at(request.expires_at),
        RequiredAttemptStatus::Expired
    );
    assert_eq!(f.store().get_required_attempt(first.id).unwrap(), success);
}

#[test]
fn failure_and_cancellation_cannot_fall_back_to_success() {
    for conclusion in [
        RequiredAttemptConclusion::Failure,
        RequiredAttemptConclusion::Cancelled,
    ] {
        let f = Fixture::new();
        let mut request = reservation();
        let now = Utc::now();
        let first = f.store().reserve_required_attempt(&request, now).unwrap();
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                now,
            )
            .unwrap();
        request.idempotency_key = Uuid::new_v4().to_string();
        let second = f.store().reserve_required_attempt(&request, now).unwrap();
        f.store()
            .complete_required_attempt(second.id, &request, conclusion, &artifacts(), now)
            .unwrap();
        let expected = match conclusion {
            RequiredAttemptConclusion::Failure => RequiredAttemptStatus::Failure,
            RequiredAttemptConclusion::Cancelled => RequiredAttemptStatus::Cancelled,
            RequiredAttemptConclusion::Success => unreachable!(),
        };
        assert_eq!(latest(&f, &request).status_at(now), expected);
    }
}

#[test]
fn replay_preserves_server_identity_fixed_expiry_and_terminal_bytes() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    assert_eq!(
        f.store()
            .reserve_required_attempt(&request, request.expires_at)
            .unwrap(),
        first
    );
    let result = f
        .store()
        .complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        )
        .unwrap();
    let mut reordered = artifacts();
    reordered.reverse();
    assert_eq!(
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &reordered,
                request.expires_at + Duration::days(1)
            )
            .unwrap(),
        result
    );
    assert!(matches!(
        f.store().complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Failure,
            &artifacts(),
            now
        ),
        Err(ForgeError::Conflict(_))
    ));
    let mut changed = artifacts();
    changed[0].bytes.push(b' ');
    assert!(matches!(
        f.store().complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &changed,
            now
        ),
        Err(ForgeError::Conflict(_))
    ));
    let conn = f.connection();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM forge_required_attempts", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM forge_required_attempt_outbox",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM forge_audit_log WHERE action = 'required_attempt'",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn each_binding_change_conflicts_even_when_inventory_counts_match() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    let mut variants = Vec::new();
    let mut altered = request.clone();
    altered.binding.commit_sha = "f".repeat(40);
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.tree_sha = "f".repeat(40);
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.context = "another/required".into();
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.publisher_id = Uuid::new_v4();
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.actor.auth_epoch += 1;
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.actor.credential_id = Uuid::new_v4();
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.runtime_sha256 = "f".repeat(64);
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.enrollment_sha256 = "f".repeat(64);
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.evidence_contract_sha256 = "f".repeat(64);
    variants.push(altered);
    let mut altered = request.clone();
    altered.binding.authority_origin = RequiredAuthorityOrigin::Ordinary;
    variants.push(altered);
    let mut altered = request.clone();
    altered.expires_at += Duration::seconds(1);
    variants.push(altered);
    for altered in variants {
        assert!(matches!(
            f.store().reserve_required_attempt(&altered, now),
            Err(ForgeError::Conflict(_))
        ));
        assert!(matches!(
            f.store().complete_required_attempt(
                first.id,
                &altered,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                now
            ),
            Err(ForgeError::Conflict(_))
        ));
    }
}

#[test]
fn contexts_commits_and_repositories_have_independent_newest_attempts() {
    let f = Fixture::new();
    let first_request = reservation();
    let first = f
        .store()
        .reserve_required_attempt(&first_request, Utc::now())
        .unwrap();
    for field in 0..3 {
        let mut request = first_request.clone();
        request.idempotency_key = Uuid::new_v4().to_string();
        match field {
            0 => request.binding.context = "other/required".into(),
            1 => request.binding.commit_sha = "f".repeat(40),
            _ => request.binding.repository_id = Uuid::new_v4(),
        }
        let other = f
            .store()
            .reserve_required_attempt(&request, Utc::now())
            .unwrap();
        assert_eq!(other.ordinal, 1);
        assert_eq!(latest(&f, &request), other);
        assert_eq!(latest(&f, &first_request), first);
    }
}

#[test]
fn terminal_transaction_failure_retains_pending_and_no_partial_artifacts() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    f.connection().execute_batch("CREATE TRIGGER fail_terminal_outbox BEFORE INSERT ON forge_required_attempt_outbox BEGIN SELECT RAISE(ABORT, 'injected outbox failure'); END;").unwrap();
    assert!(matches!(
        f.store().complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now
        ),
        Err(ForgeError::Storage(_))
    ));
    assert_eq!(f.store().get_required_attempt(first.id).unwrap(), first);
    assert_eq!(
        f.connection()
            .query_row(
                "SELECT COUNT(*) FROM forge_required_attempt_artifacts",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        f.connection()
            .query_row(
                "SELECT COUNT(*) FROM forge_audit_log WHERE action = 'required_attempt'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    f.connection()
        .execute_batch("DROP TRIGGER fail_terminal_outbox;")
        .unwrap();
    assert!(
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                now
            )
            .is_ok()
    );
}

#[test]
fn unrelated_state_save_and_restart_preserve_attempt_artifact_and_outbox() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    let completed = f
        .store()
        .complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        )
        .unwrap();
    f.core.ensure_user("unrelated").unwrap();
    assert_eq!(latest(&f, &request), completed);
    let database = f.directory.path().join("forge.sqlite");
    drop(f.core);
    let reopened = ForgeCore::open_sqlite(database).unwrap();
    assert_eq!(
        reopened
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .get_required_attempt(first.id)
            .unwrap(),
        completed
    );
}

#[test]
fn receiving_hash_detects_content_corruption_after_persistence() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    f.store()
        .complete_required_attempt(
            first.id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        )
        .unwrap();
    let mut changed = artifacts().remove(0).bytes;
    changed[0] ^= 1;
    f.connection().execute("UPDATE forge_required_attempt_artifacts SET content = ?1 WHERE attempt_id = ?2 AND name = 'receipt.json'", params![changed, first.id.to_string()]).unwrap();
    assert!(matches!(
        f.store().get_required_attempt(first.id),
        Err(ForgeError::Storage(_))
    ));
}

#[test]
fn artifact_validation_rejects_absence_duplicates_paths_empty_bytes_and_overflow() {
    assert!(receive_artifacts(&[]).is_err());
    for name in ["../receipt", "/receipt", ".", "..", "bad\nname"] {
        assert!(
            receive_artifacts(&[RequiredArtifactBytes {
                name: name.into(),
                bytes: vec![1]
            }])
            .is_err()
        );
    }
    assert!(
        receive_artifacts(&[RequiredArtifactBytes {
            name: "receipt".into(),
            bytes: vec![]
        }])
        .is_err()
    );
    let mut duplicate = artifacts();
    duplicate[1].name = duplicate[0].name.clone();
    assert!(receive_artifacts(&duplicate).is_err());
    assert!(
        receive_artifacts(&[RequiredArtifactBytes {
            name: "receipt".into(),
            bytes: vec![1; 16 * 1024 * 1024 + 1]
        }])
        .is_err()
    );
}

#[test]
fn clock_rollback_and_expiry_boundaries_refuse_new_completion() {
    let f = Fixture::new();
    let request = reservation();
    let now = Utc::now();
    let first = f.store().reserve_required_attempt(&request, now).unwrap();
    assert!(
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                now - Duration::nanoseconds(1)
            )
            .is_err()
    );
    assert!(
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                request.expires_at
            )
            .is_err()
    );
    assert!(
        f.store()
            .complete_required_attempt(
                first.id,
                &request,
                RequiredAttemptConclusion::Success,
                &artifacts(),
                request.expires_at - Duration::nanoseconds(1)
            )
            .is_ok()
    );
    let mut expired = request;
    expired.idempotency_key = Uuid::new_v4().to_string();
    assert!(
        f.store()
            .reserve_required_attempt(&expired, expired.expires_at)
            .is_err()
    );
}

#[test]
fn older_completion_never_displaces_a_newer_failed_attempt() {
    let f = Fixture::new();
    let first_request = reservation();
    let now = Utc::now();
    let first = f
        .store()
        .reserve_required_attempt(&first_request, now)
        .unwrap();
    let mut second_request = first_request.clone();
    second_request.idempotency_key = Uuid::new_v4().to_string();
    let second = f
        .store()
        .reserve_required_attempt(&second_request, now)
        .unwrap();
    let failed = f
        .store()
        .complete_required_attempt(
            second.id,
            &second_request,
            RequiredAttemptConclusion::Failure,
            &artifacts(),
            now,
        )
        .unwrap();
    f.store()
        .complete_required_attempt(
            first.id,
            &first_request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        )
        .unwrap();
    assert_eq!(latest(&f, &first_request), failed);
    assert_eq!(failed.status_at(now), RequiredAttemptStatus::Failure);
}

#[test]
fn reservation_audit_failure_rolls_back_ordinal_and_identity() {
    let f = Fixture::new();
    let request = reservation();
    f.connection().execute_batch("CREATE TRIGGER fail_reservation_audit BEFORE INSERT ON forge_audit_log WHEN NEW.action = 'required_attempt' BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END;").unwrap();
    assert!(matches!(
        f.store().reserve_required_attempt(&request, Utc::now()),
        Err(ForgeError::Storage(_))
    ));
    assert!(
        f.store()
            .latest_required_attempt(
                request.binding.repository_id,
                &request.binding.commit_sha,
                &request.binding.context
            )
            .unwrap()
            .is_none()
    );
    f.connection()
        .execute_batch("DROP TRIGGER fail_reservation_audit;")
        .unwrap();
    assert_eq!(
        f.store()
            .reserve_required_attempt(&request, Utc::now())
            .unwrap()
            .ordinal,
        1
    );
}

#[test]
fn malformed_binding_and_unsupported_integer_input_are_refused() {
    let original = reservation();
    let mut request = original.clone();
    request.binding.actor.auth_epoch = MAX_SAFE_INTEGER + 1;
    assert!(request.validate().is_err());
    let mut request = original.clone();
    request.binding.commit_sha = "a".repeat(8);
    assert!(request.validate().is_err());
    let mut request = original.clone();
    request.binding.tree_sha = "0".repeat(40);
    assert!(request.validate().is_err());
    let mut json = serde_json::to_value(&original).unwrap();
    json["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RequiredAttemptReservation>(json).is_err());
    for invalid in [
        serde_json::json!(true),
        serde_json::json!("7"),
        serde_json::json!(7.5),
    ] {
        let mut json = serde_json::to_value(&original).unwrap();
        json["binding"]["actor"]["auth_epoch"] = invalid;
        assert!(serde_json::from_value::<RequiredAttemptReservation>(json).is_err());
    }
}

#[test]
fn readback_is_one_snapshot_across_concurrent_terminal_commit() {
    use std::sync::mpsc;
    use std::time::Duration as StdDuration;

    for read_latest in [false, true] {
        let f = Fixture::new();
        f.connection()
            .execute_batch("PRAGMA journal_mode = WAL;")
            .unwrap();
        let request = reservation();
        let now = Utc::now();
        let pending = f.store().reserve_required_attempt(&request, now).unwrap();
        let (entered, observed) = mpsc::channel();
        let (release, released) = mpsc::channel();
        f.store()
            .pause_required_attempt_read(pending.id, (entered, released));
        let store = f.store().clone();
        let read_request = request.clone();
        let id = pending.id;
        let reader = std::thread::spawn(move || {
            if read_latest {
                store
                    .latest_required_attempt(
                        read_request.binding.repository_id,
                        &read_request.binding.commit_sha,
                        &read_request.binding.context,
                    )
                    .map(Option::unwrap)
            } else {
                store.get_required_attempt(id)
            }
        });
        observed.recv_timeout(StdDuration::from_secs(5)).unwrap();
        let completed = f.store().complete_required_attempt(
            id,
            &request,
            RequiredAttemptConclusion::Success,
            &artifacts(),
            now,
        );
        release.send(()).unwrap();
        assert_eq!(reader.join().unwrap().unwrap(), pending);
        assert_eq!(latest(&f, &request), completed.unwrap());
    }
}
