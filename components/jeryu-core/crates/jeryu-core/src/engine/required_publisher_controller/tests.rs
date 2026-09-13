//! Test-only service fixtures are not enrolled operators or
//! production trust implementations. Real managed-Git/HTTP and signing-gate
//! campaigns remain required before this proposed layer can be accepted.

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::RwLock;

use super::*;
use crate::core::publisher_enrollment::RequiredPublisherEnrollment;
use crate::{
    ActorCredential, CreateRepositoryRequest, ManagedGitIdentity, ObservedReviewRef, Repository,
    RequiredAuthorityOrigin, ReviewGitObservation, ReviewGitObserver, SetBranchProtectionRequest,
};

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TREE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PASSWORD: &str = "fixture strong password 5130490501483594442";

#[derive(Debug)]
struct Observer {
    root: PathBuf,
    head: RwLock<String>,
}

impl ReviewGitObserver for Observer {
    fn storage_root(&self) -> &Path {
        &self.root
    }
    fn observe(&self, target: &ReviewGitTarget) -> Result<ReviewGitObservation> {
        let observed = ObservedReviewRef {
            reference: target.source_ref.clone(),
            commit_sha: self.head.read().clone(),
            tree_sha: TREE.into(),
            identity: ManagedGitIdentity {
                storage_root: self.root.display().to_string(),
                root_device: 1,
                root_inode: 2,
                repository_path: self
                    .root
                    .join(&target.source.owner)
                    .join(format!("{}.git", target.source.name))
                    .display()
                    .to_string(),
                repository_device: 1,
                repository_inode: 3,
                git_executable: "/usr/bin/git".into(),
                git_executable_sha256: "c".repeat(64),
            },
        };
        Ok(ReviewGitObservation {
            source: observed.clone(),
            destination: observed,
        })
    }
}

#[derive(Debug)]
struct FixtureAuthority {
    custody: super::super::RequiredPublisherCustody,
    active: AtomicBool,
}

impl RequiredPublisherAuthority for FixtureAuthority {
    fn custody(&self) -> &super::super::RequiredPublisherCustody {
        &self.custody
    }
    fn validate(
        &self,
        actual: &super::super::RequiredPublisherCustody,
        _: &DurableRequiredPublisher,
        _: RequiredPublisherAction,
        _: DateTime<Utc>,
    ) -> Result<()> {
        if actual == &self.custody && self.active.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(unavailable())
        }
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    core: ForgeCore,
    repository: Repository,
    publisher: AuthenticatedActor,
    other: AuthenticatedActor,
    enrollment: DurableRequiredPublisher,
    observer: Arc<Observer>,
    authority: Arc<FixtureAuthority>,
}

fn pat(core: &ForgeCore, login: &str) -> AuthenticatedActor {
    core.create_account(login, PASSWORD, UserRole::User)
        .unwrap();
    let session = core.create_session(login, PASSWORD).unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let pat = core
        .create_personal_access_token(&actor, "fixture", None)
        .unwrap();
    core.authenticate_actor(ActorCredential::PersonalAccessToken(&pat.secret))
        .unwrap()
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let root = directory.path().join("repositories");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let observer = Arc::new(Observer {
            root: root.clone(),
            head: RwLock::new(HEAD.into()),
        });
        let core = ForgeCore::open_managed(directory.path().join("forge.sqlite"), &root)
            .unwrap()
            .with_review_git_observer(observer.clone())
            .unwrap();
        let authority = Arc::new(FixtureAuthority {
            custody: core.required_publisher_custody().unwrap(),
            active: AtomicBool::new(true),
        });
        let core = core
            .with_required_publisher_authority(authority.clone())
            .unwrap();
        let repository = core
            .create_repository(
                "owner",
                CreateRepositoryRequest {
                    name: "fixture".into(),
                    private: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let publisher = pat(&core, "publisher");
        let issuer = pat(&core, "issuer");
        let reviewer = pat(&core, "reviewer");
        let other = pat(&core, "other");
        for login in ["publisher", "issuer", "reviewer", "other"] {
            core.grant_repo_access(
                "fixture-operator",
                login,
                "owner",
                "fixture",
                RepoAccessLevel::Write,
            )
            .unwrap();
        }
        core.set_branch_protection(
            "owner",
            "fixture",
            "main",
            SetBranchProtectionRequest {
                required_status_checks: vec!["fixture/required".into()],
                enforce_admins: true,
                required_approving_review_count: 1,
                required_linear_history: true,
                ..Default::default()
            },
        )
        .unwrap();
        let binding = |actor| {
            core.validate_actor_locked(&core.runtime.state.read(), actor, true)
                .unwrap()
        };
        let now = Utc::now();
        let enrollment = RequiredPublisherEnrollment {
            schema: "jeryu.required-publisher-enrollment/v1".into(),
            publisher_id: Uuid::new_v4(),
            revision: 1,
            actor: binding(&publisher),
            issuer: binding(&issuer),
            reviewer: binding(&reviewer),
            scopes: vec![RequiredPublisherScope {
                repository_id: repository.id,
                source_ref: "refs/heads/topic".into(),
                commit_sha: HEAD.into(),
                tree_sha: TREE.into(),
                context: "fixture/required".into(),
            }],
            runtime_sha256: "c".repeat(64),
            authority_origin: RequiredAuthorityOrigin::ReviewedStagingCandidate,
            authority_contract_sha256: "d".repeat(64),
            evidence_contract_sha256: "e".repeat(64),
            issuer_acceptance_sha256: "f".repeat(64),
            reviewer_acceptance_sha256: "1".repeat(64),
            issued_at: now - Duration::seconds(1),
            expires_at: now + Duration::minutes(30),
        };
        let enrollment = core
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .install_required_publisher(&enrollment, None, Uuid::new_v4(), now)
            .unwrap();
        Self {
            _directory: directory,
            core,
            repository,
            publisher,
            other,
            enrollment,
            observer,
            authority,
        }
    }
    fn request(&self) -> ReserveRequiredAttemptRequest {
        ReserveRequiredAttemptRequest {
            publisher_id: self.enrollment.enrollment.publisher_id,
            expected_head_sha: Some(HEAD.into()),
            expected_tree_sha: Some(TREE.into()),
            context: "fixture/required".into(),
            idempotency_key: Some(Uuid::new_v4().to_string()),
            expires_at: Utc::now() + Duration::minutes(5),
        }
    }
    fn reserve(&self) -> DurableRequiredAttempt {
        self.core
            .reserve_required_attempt(&self.publisher, self.repository.id, self.request())
            .unwrap()
    }
    fn complete(
        &self,
        attempt: &DurableRequiredAttempt,
        conclusion: RequiredAttemptConclusion,
    ) -> Result<DurableRequiredAttempt> {
        self.core.complete_required_attempt(
            &self.publisher,
            self.repository.id,
            attempt.id,
            CompleteRequiredAttemptRequest {
                expected_head_sha: Some(HEAD.into()),
                expected_reservation_sha256: Some(attempt.reservation_sha256.clone()),
                conclusion,
                artifacts: vec![RequiredArtifactUpload {
                    name: "captured.log".into(),
                    bytes: b"captured fixture output".to_vec(),
                }],
            },
        )
    }
}

#[test]
fn opaque_publisher_scope_and_newest_attempt_drive_snapshot() {
    let f = Fixture::new();
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.other, f.repository.id, f.request()),
        Err(ForgeError::Forbidden(_))
    ));
    let first = f.reserve();
    f.complete(&first, RequiredAttemptConclusion::Success)
        .unwrap();
    assert!(
        f.core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap()
            .satisfied
    );
    let second = f.reserve();
    let pending = f
        .core
        .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
        .unwrap();
    assert!(!pending.satisfied);
    assert_eq!(
        pending.contexts[0].newest_attempt.as_ref().unwrap().id,
        second.id
    );
    f.complete(&second, RequiredAttemptConclusion::Failure)
        .unwrap();
    assert!(
        !f.core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap()
            .satisfied
    );
}

#[test]
fn durable_success_with_changed_enrollment_binding_cannot_satisfy_snapshot() {
    let f = Fixture::new();
    let storage = f.core.runtime.storage.as_ref().unwrap();
    for field in ["runtime", "tree", "actor", "evidence", "origin", "expiry"] {
        let mut reservation = f.enrollment.enrollment.reservation(
            &f.enrollment.enrollment.scopes[0],
            &Uuid::new_v4().to_string(),
            Utc::now() + Duration::minutes(5),
            f.enrollment.enrollment_sha256.clone(),
        );
        match field {
            "runtime" => reservation.binding.runtime_sha256 = "2".repeat(64),
            "tree" => reservation.binding.tree_sha = "3".repeat(40),
            "actor" => reservation.binding.actor = f.enrollment.enrollment.reviewer.clone(),
            "evidence" => reservation.binding.evidence_contract_sha256 = "4".repeat(64),
            "origin" => reservation.binding.authority_origin = RequiredAuthorityOrigin::Ordinary,
            "expiry" => {
                reservation.expires_at = f.enrollment.enrollment.expires_at + Duration::seconds(1)
            }
            _ => unreachable!(),
        }
        let attempt = storage
            .reserve_required_attempt(&reservation, Utc::now())
            .unwrap();
        storage
            .complete_required_attempt(
                attempt.id,
                &reservation,
                RequiredAttemptConclusion::Success,
                &[RequiredArtifactBytes {
                    name: "retained.log".into(),
                    bytes: b"retained direct-store evidence".to_vec(),
                }],
                Utc::now(),
            )
            .unwrap();
        let snapshot = f
            .core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap();
        assert!(!snapshot.satisfied, "changed {field} must refuse");
        assert_eq!(
            snapshot.contexts[0].status,
            Some(RequiredAttemptStatus::Success)
        );
        assert!(
            snapshot.contexts[0]
                .blockers
                .iter()
                .any(|blocker| blocker.contains("differs from enrolled binding")),
            "missing exact-binding blocker for {field}: {:?}",
            snapshot.contexts[0].blockers
        );
        assert!(
            matches!(
                f.complete(&attempt, RequiredAttemptConclusion::Success),
                Err(ForgeError::Conflict(_))
            ),
            "completion must also refuse changed {field}"
        );
    }
}

#[test]
fn omitted_preconditions_and_actual_head_movement_do_not_reserve() {
    let f = Fixture::new();
    let mut request = f.request();
    request.expected_head_sha = None;
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.publisher, f.repository.id, request),
        Err(ForgeError::PreconditionRequired(_))
    ));
    *f.observer.head.write() = "d".repeat(40);
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
        Err(ForgeError::Conflict(_))
    ));
    assert!(
        f.core
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .latest_required_attempt(f.repository.id, HEAD, "fixture/required")
            .unwrap()
            .is_none()
    );
}

#[test]
fn terminal_replay_preserves_receipt_while_conflicting_replay_refuses() {
    let f = Fixture::new();
    let attempt = f.reserve();
    let completed = f
        .complete(&attempt, RequiredAttemptConclusion::Success)
        .unwrap();
    *f.observer.head.write() = "d".repeat(40);
    assert_eq!(
        f.complete(&attempt, RequiredAttemptConclusion::Success)
            .unwrap(),
        completed
    );
    assert!(matches!(
        f.complete(&attempt, RequiredAttemptConclusion::Failure),
        Err(ForgeError::Conflict(_))
    ));
}

#[test]
fn current_credential_revocation_fences_cached_publisher_and_snapshot() {
    let f = Fixture::new();
    let attempt = f.reserve();
    f.complete(&attempt, RequiredAttemptConclusion::Success)
        .unwrap();
    f.core
        .revoke_personal_access_token("publisher", f.enrollment.enrollment.actor.credential_id)
        .unwrap();
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
        Err(ForgeError::Unauthenticated(_))
    ));
    assert!(matches!(
        f.complete(&attempt, RequiredAttemptConclusion::Success),
        Err(ForgeError::Unauthenticated(_))
    ));
    assert!(
        !f.core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap()
            .satisfied
    );
}

#[test]
fn unavailable_installation_never_falls_back_to_legacy_rows() {
    let f = Fixture::new();
    let attempt = f.reserve();
    f.complete(&attempt, RequiredAttemptConclusion::Success)
        .unwrap();
    f.authority.active.store(false, Ordering::SeqCst);
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(
        !f.core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap()
            .satisfied
    );
}

#[test]
fn enrollment_expected_state_and_idempotency_are_exact() {
    let f = Fixture::new();
    let storage = f.core.runtime.storage.as_ref().unwrap();
    assert_eq!(
        storage
            .install_required_publisher(
                &f.enrollment.enrollment,
                None,
                f.enrollment.installation_operation_id,
                f.enrollment.enrollment.expires_at + Duration::seconds(1),
            )
            .unwrap(),
        f.enrollment
    );
    assert!(matches!(
        storage.install_required_publisher(
            &f.enrollment.enrollment,
            Some(&"0".repeat(64)),
            f.enrollment.installation_operation_id,
            Utc::now(),
        ),
        Err(ForgeError::Conflict(_))
    ));
    let mut successor = f.enrollment.enrollment.clone();
    successor.revision += 1;
    assert!(matches!(
        storage.install_required_publisher(&successor, None, Uuid::new_v4(), Utc::now()),
        Err(ForgeError::Conflict(_))
    ));
    assert!(matches!(
        storage.install_required_publisher(
            &successor,
            Some(&"0".repeat(64)),
            Uuid::new_v4(),
            Utc::now()
        ),
        Err(ForgeError::Conflict(_))
    ));
    let mut conflicting = f.enrollment.enrollment.clone();
    conflicting.scopes[0].context = "different/required".into();
    assert!(matches!(
        storage.install_required_publisher(
            &conflicting,
            None,
            f.enrollment.installation_operation_id,
            Utc::now(),
        ),
        Err(ForgeError::Conflict(_))
    ));
}

#[test]
fn enrollment_rotation_fences_old_success_and_pending_completion() {
    let f = Fixture::new();
    let success = f.reserve();
    f.complete(&success, RequiredAttemptConclusion::Success)
        .unwrap();
    let pending = f.reserve();
    let mut successor = f.enrollment.enrollment.clone();
    successor.revision += 1;
    let installed = f
        .core
        .runtime
        .storage
        .as_ref()
        .unwrap()
        .install_required_publisher(
            &successor,
            Some(&f.enrollment.enrollment_sha256),
            Uuid::new_v4(),
            Utc::now(),
        )
        .unwrap();
    assert_ne!(installed.enrollment_sha256, f.enrollment.enrollment_sha256);
    assert!(matches!(
        f.complete(&pending, RequiredAttemptConclusion::Success),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        f.complete(&success, RequiredAttemptConclusion::Success),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(
        !f.core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap()
            .satisfied
    );
    let next = f.reserve();
    assert_eq!(
        next.reservation.binding.enrollment_sha256,
        installed.enrollment_sha256
    );
}

#[test]
fn targeted_revocation_is_immutable_and_refuses_new_effects() {
    let f = Fixture::new();
    let attempt = f.reserve();
    let actor = f
        .core
        .validate_actor_locked(&f.core.runtime.state.read(), &f.other, true)
        .unwrap();
    let revocation = crate::core::publisher_enrollment::RequiredPublisherRevocation {
        operation_id: Uuid::new_v4(),
        actor,
        revoked_at: Utc::now(),
        reason: "fixture revocation".into(),
    };
    let storage = f.core.runtime.storage.as_ref().unwrap();
    let revoked = storage
        .revoke_required_publisher(
            f.enrollment.enrollment.publisher_id,
            &f.enrollment.enrollment_sha256,
            &revocation,
        )
        .unwrap();
    assert_eq!(
        storage
            .revoke_required_publisher(
                f.enrollment.enrollment.publisher_id,
                &f.enrollment.enrollment_sha256,
                &revocation
            )
            .unwrap(),
        revoked
    );
    let mut different = revocation.clone();
    different.reason = "different request".into();
    assert!(matches!(
        storage.revoke_required_publisher(
            f.enrollment.enrollment.publisher_id,
            &f.enrollment.enrollment_sha256,
            &different
        ),
        Err(ForgeError::Conflict(_))
    ));
    assert!(matches!(
        f.core
            .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        f.complete(&attempt, RequiredAttemptConclusion::Success),
        Err(ForgeError::Forbidden(_))
    ));
}

#[test]
fn enrollment_audit_failure_rolls_back_installation_then_retries_once() {
    let f = Fixture::new();
    let database = f._directory.path().join("forge.sqlite");
    let conn = rusqlite::Connection::open(database).unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_enrollment_audit BEFORE INSERT ON forge_audit_log WHEN NEW.action = 'required_publisher_enrollment' BEGIN SELECT RAISE(ABORT, 'fixture audit refused'); END;").unwrap();
    let mut enrollment = f.enrollment.enrollment.clone();
    enrollment.publisher_id = Uuid::new_v4();
    let operation = Uuid::new_v4();
    let now = Utc::now();
    let storage = f.core.runtime.storage.as_ref().unwrap();
    assert!(
        storage
            .install_required_publisher(&enrollment, None, operation, now)
            .is_err()
    );
    assert!(
        storage
            .latest_required_publisher(enrollment.publisher_id)
            .unwrap()
            .is_none()
    );
    conn.execute_batch("DROP TRIGGER refuse_enrollment_audit;")
        .unwrap();
    let installed = storage
        .install_required_publisher(&enrollment, None, operation, now)
        .unwrap();
    assert_eq!(
        storage
            .install_required_publisher(&enrollment, None, operation, now)
            .unwrap(),
        installed
    );
    let count: u64 = conn.query_row("SELECT COUNT(*) FROM forge_audit_log WHERE action = 'required_publisher_enrollment' AND subject = ?1", [enrollment.publisher_id.to_string()], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn durable_enrollment_survives_unrelated_save_and_reopen_without_reactivating_gate() {
    let f = Fixture::new();
    let database = f._directory.path().join("forge.sqlite");
    let root = f._directory.path().join("repositories");
    let expected = f.enrollment.clone();
    f.core
        .create_repository(
            "other",
            CreateRepositoryRequest {
                name: "unrelated".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let Fixture {
        _directory, core, ..
    } = f;
    drop(core);
    let reopened = ForgeCore::open_managed(&database, &root).unwrap();
    assert_eq!(
        reopened
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .latest_required_publisher(expected.enrollment.publisher_id)
            .unwrap()
            .unwrap(),
        expected
    );
    assert!(matches!(
        reopened.required_authority(),
        Err(ForgeError::WriterUnavailable(_))
    ));
    drop(reopened);
    drop(_directory);
}

#[cfg(unix)]
#[test]
fn each_operation_rechecks_actual_custody_under_existing_guards() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for resource in 0..4 {
        let f = Fixture::new();
        let successful = f.reserve();
        f.complete(&successful, RequiredAttemptConclusion::Success)
            .unwrap();
        let measured = f.core.required_publisher_custody().unwrap();
        let path = match resource {
            0 => &measured.database.resource.path,
            1 => &measured.storage_root.path,
            _ => &measured.writer_leases[resource - 2].resource.path,
        };
        let original = std::fs::metadata(path).unwrap().mode();
        // Group-read alone remains compatible with the cooperative writer lease,
        // but differs from the exact publisher installation incarnation.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(original ^ 0o040)).unwrap();
        assert!(
            matches!(
                f.core
                    .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "reserve resource {resource}"
        );
        assert!(
            matches!(
                f.complete(&successful, RequiredAttemptConclusion::Success),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "terminal replay resource {resource}"
        );
        let snapshot = f
            .core
            .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
            .unwrap();
        assert!(!snapshot.satisfied, "snapshot resource {resource}");
        assert_eq!(
            snapshot.contexts[0].status,
            Some(RequiredAttemptStatus::Success)
        );
        assert!(
            snapshot.contexts[0]
                .blockers
                .iter()
                .any(|reason| reason.contains("does not bind this database"))
        );
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(original)).unwrap();
        assert!(
            f.core
                .required_attempt_snapshot(&f.other, f.repository.id, HEAD)
                .unwrap()
                .satisfied
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn released_live_lock_refuses_all_publisher_routes_without_reacquisition() {
    for resource in 0..3 {
        let f = Fixture::new();
        let attempt = f.reserve();
        let before = f.core.required_publisher_custody().unwrap();
        let held = if resource == 0 {
            &before.database
        } else {
            &before.writer_leases[resource - 1]
        };
        f.core
            .runtime
            .storage
            .as_ref()
            .unwrap()
            .release_writer_lock_for_test(resource)
            .unwrap();
        assert!(
            matches!(
                f.core.required_publisher_custody(),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "readback resource {resource}"
        );
        assert!(
            matches!(
                f.core
                    .clone()
                    .with_required_publisher_authority(f.authority.clone()),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "attachment resource {resource}"
        );
        assert!(
            matches!(
                f.core
                    .reserve_required_attempt(&f.publisher, f.repository.id, f.request()),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "reserve resource {resource}"
        );
        assert!(
            matches!(
                f.complete(&attempt, RequiredAttemptConclusion::Success),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "complete resource {resource}"
        );
        assert!(
            matches!(
                f.core
                    .required_attempt_snapshot(&f.other, f.repository.id, HEAD),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "snapshot resource {resource}"
        );
        let metadata = std::fs::metadata(format!("/proc/self/fd/{}", held.descriptor)).unwrap();
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            (metadata.dev(), metadata.ino()),
            (held.resource.device, held.resource.inode)
        );
        let fdinfo =
            std::fs::read_to_string(format!("/proc/self/fdinfo/{}", held.descriptor)).unwrap();
        assert!(
            !fdinfo
                .lines()
                .any(|line| line.starts_with("lock:") && line.contains("FLOCK")),
            "broken custody was silently reacquired"
        );
    }
}
