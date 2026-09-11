#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Barrier, mpsc};
use std::time::{Duration as StdDuration, Instant};

use parking_lot::{Mutex, RwLock};
use rusqlite::Connection;

use super::*;

const PASSWORD: &str = "correct horse battery";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TREE: &str = "cccccccccccccccccccccccccccccccccccccccc";
const OTHER: &str = "dddddddddddddddddddddddddddddddddddddddd";

/// Test-only observer injection exercises Core's admission independently of
/// request data. The production Gitd adapter has separate real-Git tests.
#[derive(Debug)]
struct Observer {
    root: PathBuf,
    head: RwLock<String>,
    base: RwLock<String>,
    tree: RwLock<String>,
    error: Mutex<Option<ForgeError>>,
    pause: Mutex<Option<(mpsc::Sender<()>, Arc<Barrier>)>>,
}

impl ReviewGitObserver for Observer {
    fn storage_root(&self) -> &Path {
        &self.root
    }
    fn observe(&self, target: &ReviewGitTarget) -> Result<ReviewGitObservation> {
        if let Some((entered, release)) = self.pause.lock().take() {
            entered.send(()).unwrap();
            release.wait();
        }
        if let Some(error) = self.error.lock().clone() {
            return Err(error);
        }
        let observed =
            |repo: &ReviewGitRepository, reference: &str, head: String| ObservedReviewRef {
                reference: reference.into(),
                commit_sha: head,
                tree_sha: self.tree.read().clone(),
                identity: ManagedGitIdentity {
                    storage_root: self.root.to_string_lossy().into(),
                    root_device: 1,
                    root_inode: 2,
                    repository_path: self
                        .root
                        .join(&repo.owner)
                        .join(format!("{}.git", repo.name))
                        .to_string_lossy()
                        .into(),
                    repository_device: 1,
                    repository_inode: 3,
                    git_executable: "/usr/bin/git".into(),
                    git_executable_sha256: "e".repeat(64),
                },
            };
        Ok(ReviewGitObservation {
            source: observed(&target.source, &target.source_ref, self.head.read().clone()),
            destination: observed(
                &target.destination,
                &target.destination_ref,
                self.base.read().clone(),
            ),
        })
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    database: PathBuf,
    core: ForgeCore,
    observer: Arc<Observer>,
    repo: Repository,
    pr: PullRequest,
}

fn fixture() -> Fixture {
    let directory = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path().join("repos");
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let database = directory.path().join("forge.sqlite");
    let observer = Arc::new(Observer {
        root: root.clone(),
        head: RwLock::new(HEAD.into()),
        base: RwLock::new(BASE.into()),
        tree: RwLock::new(TREE.into()),
        error: Mutex::new(None),
        pause: Mutex::new(None),
    });
    let core = ForgeCore::open_managed(&database, &root)
        .unwrap()
        .with_review_git_observer(observer.clone())
        .unwrap();
    let repo = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "demo".into(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    for login in ["author", "reviewer", "second"] {
        core.create_account(login, PASSWORD, UserRole::User)
            .unwrap();
        core.grant_repo_access("operator", login, "owner", "demo", RepoAccessLevel::Write)
            .unwrap();
    }
    core.set_branch_protection(
        "owner",
        "demo",
        "main",
        SetBranchProtectionRequest {
            required_approving_review_count: 1,
            required_status_checks: vec!["demo/required".into()],
            enforce_admins: true,
            required_linear_history: true,
            ..Default::default()
        },
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "owner",
            "demo",
            "author",
            CreatePullRequestRequest {
                title: "reviewed change".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(HEAD.into()),
                base_sha: Some(BASE.into()),
                ..Default::default()
            },
        )
        .unwrap();
    Fixture {
        _directory: directory,
        database,
        core,
        observer,
        repo,
        pr,
    }
}

fn session_actor(core: &ForgeCore, login: &str) -> (SessionReceipt, AuthenticatedActor) {
    let session = core.create_session(login, PASSWORD).unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    (session, actor)
}

fn request(challenge: &ReviewChallenge, event: ReviewState) -> SubmitBoundReviewRequest {
    SubmitBoundReviewRequest {
        challenge_id: challenge.id,
        nonce: challenge.nonce.clone(),
        expected_head_sha: challenge.snapshot.git.source.commit_sha.clone(),
        event,
        body: Some("review body".into()),
        comments: vec![],
    }
}

fn submit(f: &Fixture, actor: &AuthenticatedActor, event: ReviewState) -> BoundReviewEvent {
    let challenge = f
        .core
        .create_review_challenge(actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    f.core
        .submit_bound_review(actor, f.repo.id, f.pr.number, request(&challenge, event))
        .unwrap()
}

#[test]
fn password_issuance_and_actor_capabilities_cannot_be_reconstructed_from_names() {
    let f = fixture();
    assert!(matches!(
        f.core.create_session("reviewer", "wrong password"),
        Err(ForgeError::Unauthenticated(_))
    ));
    let (session, actor) = session_actor(&f.core, "reviewer");
    assert!(!format!("{actor:?}").contains(&session.token));
    let readonly = f
        .core
        .authenticate_actor(ActorCredential::SessionReadOnly(&session.token))
        .unwrap();
    assert!(
        f.core
            .bound_review_history(&readonly, f.repo.id, f.pr.number)
            .is_ok()
    );
    assert!(matches!(
        f.core
            .create_personal_access_token(&readonly, "forged", None),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        f.core
            .create_review_challenge(&readonly, f.repo.id, f.pr.number, HEAD),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(matches!(
        f.core.authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: "wrong"
        }),
        Err(ForgeError::Forbidden(_))
    ));
    let other = ForgeCore::new();
    assert!(matches!(
        other.create_personal_access_token(&actor, "cross-runtime", None),
        Err(ForgeError::Unauthenticated(_))
    ));
    f.core
        .reset_account_password("reviewer", "replacement password")
        .unwrap();
    assert!(matches!(
        f.core.create_session("reviewer", PASSWORD),
        Err(ForgeError::Unauthenticated(_))
    ));
    assert!(matches!(
        f.core.create_personal_access_token(&actor, "revoked", None),
        Err(ForgeError::Unauthenticated(_))
    ));
    let temporary = f
        .core
        .create_session("reviewer", "replacement password")
        .unwrap();
    assert!(matches!(
        f.core
            .authenticate_actor(ActorCredential::SessionReadOnly(&temporary.token)),
        Err(ForgeError::Unauthenticated(_))
    ));
}

#[test]
fn individual_credential_revocation_fences_cached_actors_at_the_same_epoch() {
    let f = fixture();
    let (session, actor) = session_actor(&f.core, "reviewer");
    let pat = f
        .core
        .create_personal_access_token(&actor, "review", None)
        .unwrap();
    let pat_actor = f
        .core
        .authenticate_actor(ActorCredential::PersonalAccessToken(&pat.secret))
        .unwrap();
    let epoch = f.core.get_account("reviewer").unwrap().auth_epoch;
    let challenge = f
        .core
        .create_review_challenge(&pat_actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    f.core
        .revoke_personal_access_token("reviewer", pat.token.id)
        .unwrap();
    assert_eq!(f.core.get_account("reviewer").unwrap().auth_epoch, epoch);
    assert!(matches!(
        f.core.submit_bound_review(
            &pat_actor,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::Unauthenticated(_))
    ));
    f.core.revoke_session(&session.token).unwrap();
    assert!(matches!(
        f.core
            .create_personal_access_token(&actor, "reissued", None),
        Err(ForgeError::Unauthenticated(_))
    ));
    assert!(
        f.core
            .list_reviews("owner", "demo", f.pr.number)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn review_nonce_event_comments_and_retry_survive_a_real_sqlite_reopen() {
    let f = fixture();
    let (session, actor) = session_actor(&f.core, "reviewer");
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    assert_eq!(challenge.snapshot.git.source.commit_sha, HEAD);
    assert_eq!(challenge.snapshot.git.destination.commit_sha, BASE);
    assert_eq!(challenge.snapshot.git.source.tree_sha, TREE);
    assert!(!challenge.merge_qualified);
    let mut request = request(&challenge, ReviewState::Approved);
    request.comments.push(ReviewCommentInput {
        path: "src/lib.rs".into(),
        line: Some(7),
        body: "checked this behavior".into(),
    });
    let event = f
        .core
        .submit_bound_review(&actor, f.repo.id, f.pr.number, request.clone())
        .unwrap();
    assert_eq!(event.sequence, 1);
    assert_eq!(event.comments.len(), 1);
    assert_eq!(
        f.core
            .list_review_comments("owner", "demo", f.pr.number)
            .unwrap(),
        event.comments
    );
    f.core.ensure_user("unrelated-profile").unwrap();
    let repo_id = f.repo.id;
    let number = f.pr.number;
    let Fixture {
        core,
        observer,
        database,
        _directory,
        ..
    } = f;
    drop(core);
    let reopened = ForgeCore::open_managed(&database, &observer.root)
        .unwrap()
        .with_review_git_observer(observer.clone())
        .unwrap();
    assert!(matches!(
        reopened.bound_review_history(&actor, repo_id, number),
        Err(ForgeError::Unauthenticated(_))
    ));
    let new_actor = reopened
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    // Accepted replay returns its original immutable result despite newer Git.
    *observer.base.write() = OTHER.into();
    let retry = reopened
        .submit_bound_review(&new_actor, repo_id, number, request.clone())
        .unwrap();
    assert_eq!(event, retry);
    request.body = Some("different decision body".into());
    assert!(matches!(
        reopened.submit_bound_review(&new_actor, repo_id, number, request),
        Err(ForgeError::Conflict(_))
    ));
    let history = reopened
        .bound_review_history(&new_actor, repo_id, number)
        .unwrap();
    assert_eq!(history.events, vec![event.clone()]);
    assert_eq!(history.qualification.effective_reviews, vec![event.review]);
    assert!(!history.qualification.merge_qualified);
    assert_eq!(
        reopened
            .list_audit(&repo_id.to_string())
            .unwrap()
            .iter()
            .filter(|entry| entry.action == "review.bound_event")
            .count(),
        1
    );
}

#[test]
fn missing_wrong_and_changed_git_inputs_never_create_bound_events() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    for head in [
        "short",
        "0000000000000000000000000000000000000000",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        assert!(matches!(
            f.core
                .create_review_challenge(&actor, f.repo.id, f.pr.number, head),
            Err(ForgeError::Validation(_))
        ));
    }
    assert!(matches!(
        f.core
            .create_review_challenge(&actor, f.repo.id, f.pr.number, OTHER),
        Err(ForgeError::Conflict(_))
    ));
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    *f.observer.base.write() = OTHER.into();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    *f.observer.base.write() = BASE.into();
    *f.observer.tree.write() = OTHER.into();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    *f.observer.error.lock() = Some(ForgeError::WriterUnavailable("actual Git error".into()));
    assert!(matches!(
        f.core
            .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(
        f.core
            .bound_review_history(&actor, f.repo.id, f.pr.number)
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
fn nonce_actor_expiry_and_self_approval_are_enforced_inside_core() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let (_, other) = session_actor(&f.core, "second");
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    let mut wrong = request(&challenge, ReviewState::Approved);
    wrong.nonce.push('x');
    assert!(matches!(
        f.core
            .submit_bound_review(&actor, f.repo.id, f.pr.number, wrong),
        Err(ForgeError::Conflict(_))
    ));
    assert!(matches!(
        f.core.submit_bound_review(
            &other,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    let mut expired = challenge.clone();
    expired.id = Uuid::new_v4();
    expired.expires_at = Utc::now() - Duration::seconds(1);
    f.core
        .review_storage()
        .unwrap()
        .insert_review_challenge(&expired)
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&expired, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    let (_, author) = session_actor(&f.core, "author");
    let own = f
        .core
        .create_review_challenge(&author, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &author,
            f.repo.id,
            f.pr.number,
            request(&own, ReviewState::Approved)
        ),
        Err(ForgeError::Forbidden(_))
    ));
    assert!(
        f.core
            .bound_review_history(&actor, f.repo.id, f.pr.number)
            .unwrap()
            .events
            .is_empty()
    );
}

#[test]
fn policy_and_check_changes_invalidate_the_exact_pending_evidence_snapshot() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let first = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    f.core
        .set_branch_protection(
            "owner",
            "demo",
            "main",
            SetBranchProtectionRequest {
                required_approving_review_count: 2,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&first, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    let second = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    f.core
        .create_check_run(
            "owner",
            "demo",
            CreateCheckRunRequest {
                name: "demo/required".into(),
                head_sha: HEAD.into(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Success),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&second, ReviewState::Approved)
        ),
        Err(ForgeError::Conflict(_))
    ));
    let current = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    assert_eq!(current.snapshot.advisory_check_runs.len(), 1);
    assert!(!current.merge_qualified);
}

#[test]
fn comments_preserve_rejection_and_targeted_dismissal_never_resurrects_approval() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let approved = submit(&f, &actor, ReviewState::Approved);
    let rejected = submit(&f, &actor, ReviewState::ChangesRequested);
    submit(&f, &actor, ReviewState::Commented);
    assert_eq!(
        f.core
            .review_qualification("owner", "demo", f.pr.number)
            .unwrap()
            .effective_reviews,
        vec![rejected.review.clone()]
    );
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    let mut dismissal = DismissBoundReviewRequest {
        challenge_id: challenge.id,
        nonce: challenge.nonce.clone(),
        expected_head_sha: HEAD.into(),
        review_id: approved.id,
        reason: "withdraw decision".into(),
    };
    assert!(matches!(
        f.core
            .dismiss_bound_review(&actor, f.repo.id, f.pr.number, dismissal.clone()),
        Err(ForgeError::Conflict(_))
    ));
    dismissal.review_id = rejected.id;
    let event = f
        .core
        .dismiss_bound_review(&actor, f.repo.id, f.pr.number, dismissal)
        .unwrap();
    assert_eq!(event.sequence, 4);
    assert_eq!(event.review.dismissed_review_id, Some(rejected.id));
    assert!(
        f.core
            .review_qualification("owner", "demo", f.pr.number)
            .unwrap()
            .effective_reviews
            .is_empty()
    );
    assert_eq!(
        f.core
            .list_reviews("owner", "demo", f.pr.number)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn reviewer_permission_changes_block_pending_use_without_erasing_rejections() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let rejection = submit(&f, &actor, ReviewState::ChangesRequested);
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    f.core
        .grant_repo_access(
            "operator",
            "reviewer",
            "owner",
            "demo",
            RepoAccessLevel::Read,
        )
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::Forbidden(_))
    ));
    f.core.disable_account("reviewer").unwrap();
    assert_eq!(
        f.core
            .review_qualification("owner", "demo", f.pr.number)
            .unwrap()
            .effective_reviews,
        vec![rejection.review]
    );
}

#[test]
fn transaction_failure_rolls_back_comments_verdict_audit_and_nonce_then_retries_once() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    let mut request = request(&challenge, ReviewState::Approved);
    request.comments.push(ReviewCommentInput {
        path: "src/lib.rs".into(),
        line: Some(1),
        body: "retained after retry".into(),
    });
    let connection = Connection::open(&f.database).unwrap();
    connection.execute_batch("CREATE TRIGGER refuse_review_audit BEFORE INSERT ON forge_audit_log WHEN NEW.action = 'review.bound_event' BEGIN SELECT RAISE(ABORT, 'injected review commit failure'); END;").unwrap();
    assert!(
        matches!(f.core.submit_bound_review(&actor, f.repo.id, f.pr.number, request.clone()), Err(ForgeError::Storage(ref reason)) if reason.contains("injected review commit failure"))
    );
    assert!(
        f.core
            .list_reviews("owner", "demo", f.pr.number)
            .unwrap()
            .is_empty()
    );
    assert!(
        f.core
            .list_review_comments("owner", "demo", f.pr.number)
            .unwrap()
            .is_empty()
    );
    assert!(
        f.core
            .bound_review_history(&actor, f.repo.id, f.pr.number)
            .unwrap()
            .events
            .is_empty()
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM reviews", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM forge_bound_review_events",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert!(
        f.core
            .review_storage()
            .unwrap()
            .load_review_challenge(challenge.id)
            .unwrap()
            .1
            .is_none()
    );
    connection
        .execute_batch("DROP TRIGGER refuse_review_audit;")
        .unwrap();
    let event = f
        .core
        .submit_bound_review(&actor, f.repo.id, f.pr.number, request)
        .unwrap();
    assert_eq!(event.sequence, 1);
    assert_eq!(event.comments.len(), 1);
}

#[test]
fn historical_login_reviews_and_successful_legacy_checks_are_advisory() {
    let f = fixture();
    f.core
        .create_review(
            "owner",
            "demo",
            f.pr.number,
            "reviewer",
            CreateReviewRequest {
                event: ReviewState::Approved,
                body: None,
                comments: vec![],
                expected_head_sha: Some(HEAD.into()),
            },
        )
        .unwrap();
    f.core
        .create_commit_status(
            "owner",
            "demo",
            HEAD,
            "reviewer",
            CreateCommitStatusRequest {
                state: CommitStatusState::Success,
                context: "demo/required".into(),
                description: None,
                target_url: None,
            },
        )
        .unwrap();
    let projection = f
        .core
        .review_qualification("owner", "demo", f.pr.number)
        .unwrap();
    assert!(projection.effective_reviews.is_empty());
    assert_eq!(projection.advisory_reviews.len(), 1);
    let evaluation = f
        .core
        .evaluate_pull_request("owner", "demo", f.pr.number, Some(HEAD))
        .unwrap();
    assert!(
        evaluation
            .blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::MissingReview { .. }))
    );
    assert!(
        evaluation
            .blockers
            .iter()
            .any(|b| matches!(b, MergeBlocker::MissingStatusCheck { .. }))
    );
    let before = f
        .core
        .get_pull_request("owner", "demo", f.pr.number)
        .unwrap();
    assert!(matches!(
        f.core
            .evaluate_merge_readiness("owner", "demo", f.pr.number, Some(HEAD)),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(matches!(
        f.core
            .finalize_merge("owner", "demo", f.pr.number, HEAD.into(), Some(HEAD)),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert!(matches!(
        f.core.merge_pull_request(
            "owner",
            "demo",
            f.pr.number,
            MergePullRequestRequest::default()
        ),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert_eq!(
        f.core
            .get_pull_request("owner", "demo", f.pr.number)
            .unwrap(),
        before
    );
    assert!(
        f.core
            .get_issue("owner", "demo", f.pr.issue_number)
            .unwrap()
            .closed_at
            .is_none()
    );
}

#[test]
fn managed_observer_is_shared_and_cannot_be_replaced_by_another_core_handle() {
    let f = fixture();
    let other = ForgeCore::open_managed(&f.database, &f.observer.root).unwrap();
    assert!(
        other
            .clone()
            .with_review_git_observer(f.observer.clone())
            .is_ok()
    );
    let replacement = Arc::new(Observer {
        root: f.observer.root.clone(),
        head: RwLock::new(OTHER.into()),
        base: RwLock::new(BASE.into()),
        tree: RwLock::new(TREE.into()),
        error: Mutex::new(None),
        pause: Mutex::new(None),
    });
    assert!(matches!(
        other.with_review_git_observer(replacement),
        Err(ForgeError::Conflict(_))
    ));
    assert!(matches!(
        ForgeCore::new().with_review_git_observer(f.observer.clone()),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn credential_revocation_cannot_enter_while_real_review_observation_holds_guards() {
    let f = fixture();
    let (session, actor) = session_actor(&f.core, "reviewer");
    let release = Arc::new(Barrier::new(2));
    let (entered_tx, entered_rx) = mpsc::channel();
    *f.observer.pause.lock() = Some((entered_tx, release.clone()));
    let worker = f.core.clone();
    let repo_id = f.repo.id;
    let number = f.pr.number;
    let review_thread =
        std::thread::spawn(move || worker.create_review_challenge(&actor, repo_id, number, HEAD));
    entered_rx.recv_timeout(StdDuration::from_secs(5)).unwrap();
    let before = f.core.coordinator().admission_entries();
    let revoker = f.core.clone();
    let (result_tx, result_rx) = mpsc::channel();
    let revoke_thread = std::thread::spawn(move || {
        result_tx
            .send(revoker.revoke_session(&session.token))
            .unwrap();
    });
    let deadline = Instant::now() + StdDuration::from_secs(5);
    while f.core.coordinator().admission_entries() == before {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(matches!(
        result_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    release.wait();
    review_thread.join().unwrap().unwrap();
    result_rx
        .recv_timeout(StdDuration::from_secs(5))
        .unwrap()
        .unwrap();
    revoke_thread.join().unwrap();
}

#[test]
fn caller_head_precondition_cannot_be_substituted_by_a_valid_challenge() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    let mut wrong = request(&challenge, ReviewState::Approved);
    wrong.expected_head_sha = OTHER.into();
    assert!(matches!(
        f.core
            .submit_bound_review(&actor, f.repo.id, f.pr.number, wrong),
        Err(ForgeError::Conflict(_))
    ));
    assert!(
        f.core
            .bound_review_history(&actor, f.repo.id, f.pr.number)
            .unwrap()
            .events
            .is_empty()
    );
    let approved = f
        .core
        .submit_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            request(&challenge, ReviewState::Approved),
        )
        .unwrap();
    let dismiss = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    assert!(matches!(
        f.core.dismiss_bound_review(
            &actor,
            f.repo.id,
            f.pr.number,
            DismissBoundReviewRequest {
                challenge_id: dismiss.id,
                nonce: dismiss.nonce,
                expected_head_sha: OTHER.into(),
                review_id: approved.id,
                reason: "wrong head".into(),
            }
        ),
        Err(ForgeError::Conflict(_))
    ));
    assert_eq!(
        f.core
            .review_qualification("owner", "demo", f.pr.number)
            .unwrap()
            .effective_reviews,
        vec![approved.review]
    );
}

#[test]
fn fork_source_requires_catalog_custody_and_current_read_access() {
    let f = fixture();
    let source = f
        .core
        .create_repository(
            "fork",
            CreateRepositoryRequest {
                name: "source".into(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    let pr = f
        .core
        .create_pull_request(
            "owner",
            "demo",
            "author",
            CreatePullRequestRequest {
                title: "fork change".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(HEAD.into()),
                base_sha: Some(BASE.into()),
                source_repository: Some("fork/source".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let (_, actor) = session_actor(&f.core, "reviewer");
    assert!(matches!(
        f.core
            .create_review_challenge(&actor, f.repo.id, pr.number, HEAD),
        Err(ForgeError::Forbidden(_))
    ));
    f.core
        .grant_repo_access(
            "operator",
            "reviewer",
            "fork",
            "source",
            RepoAccessLevel::Read,
        )
        .unwrap();
    let challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, pr.number, HEAD)
        .unwrap();
    assert_eq!(challenge.snapshot.target.source.id, source.id);
    assert_eq!(challenge.snapshot.target.destination.id, f.repo.id);
    f.core
        .block_repository_mutations(
            source.id,
            RepositoryMutationBlock::ReconciliationRequired {
                operation_id: Uuid::new_v4(),
                reason: "unexplained Git result".into(),
            },
        )
        .unwrap();
    assert!(matches!(
        f.core.submit_bound_review(
            &actor,
            f.repo.id,
            pr.number,
            request(&challenge, ReviewState::Approved)
        ),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn review_events_and_audit_survive_intentional_catalog_deletion() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let event = submit(&f, &actor, ReviewState::Approved);
    f.core.delete_repository("owner", "demo").unwrap();
    let connection = Connection::open(&f.database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM forge_bound_review_events",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    let payload: String = connection
        .query_row(
            "SELECT event_json FROM forge_bound_review_events",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<BoundReviewEvent>(&payload).unwrap(),
        event
    );
    assert_eq!(
        f.core
            .list_audit(&f.repo.id.to_string())
            .unwrap()
            .iter()
            .filter(|entry| entry.id == event.audit_id.to_string())
            .count(),
        1
    );
    assert!(
        connection
            .execute("DELETE FROM forge_bound_review_events", [])
            .is_err()
    );
}

#[test]
fn challenge_expiry_is_rechecked_after_blocking_git_observation() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let mut challenge = f
        .core
        .create_review_challenge(&actor, f.repo.id, f.pr.number, HEAD)
        .unwrap();
    // Seed a short-lived historical challenge through its private persistence
    // boundary; the public API keeps its fixed ten-minute lifetime.
    challenge.id = Uuid::new_v4();
    challenge.expires_at = Utc::now() + Duration::seconds(2);
    f.core
        .review_storage()
        .unwrap()
        .insert_review_challenge(&challenge)
        .unwrap();
    let expires_at = challenge.expires_at;
    let release = Arc::new(Barrier::new(2));
    let (entered_tx, entered_rx) = mpsc::channel();
    *f.observer.pause.lock() = Some((entered_tx, release.clone()));
    let worker = f.core.clone();
    let repository_id = f.repo.id;
    let number = f.pr.number;
    let handle = std::thread::spawn(move || {
        worker.submit_bound_review(
            &actor,
            repository_id,
            number,
            request(&challenge, ReviewState::Approved),
        )
    });
    entered_rx.recv_timeout(StdDuration::from_secs(5)).unwrap();
    while Utc::now() <= expires_at {
        std::thread::sleep(StdDuration::from_millis(10));
    }
    release.wait();
    assert!(
        matches!(handle.join().unwrap(), Err(ForgeError::Conflict(ref reason)) if reason.contains("expired during"))
    );
    assert!(
        f.core
            .list_reviews("owner", "demo", number)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn stored_approval_withdraws_on_exact_credential_revocation_or_expiry() {
    for case in [
        "pat-revoke",
        "pat-expire",
        "session-revoke",
        "session-expire",
    ] {
        let f = fixture();
        let (session, session_bound_actor) = session_actor(&f.core, "reviewer");
        let pat = f
            .core
            .create_personal_access_token(&session_bound_actor, "review", None)
            .unwrap();
        let reviewer = if case.starts_with("pat") {
            f.core
                .authenticate_actor(ActorCredential::PersonalAccessToken(&pat.secret))
                .unwrap()
        } else {
            session_bound_actor
        };
        let epoch = f.core.get_account("reviewer").unwrap().auth_epoch;
        let challenge = f
            .core
            .create_review_challenge(&reviewer, f.repo.id, f.pr.number, HEAD)
            .unwrap();
        let event = f
            .core
            .submit_bound_review(
                &reviewer,
                f.repo.id,
                f.pr.number,
                request(&challenge, ReviewState::Approved),
            )
            .unwrap();
        assert_eq!(
            f.core
                .review_qualification("owner", "demo", f.pr.number)
                .unwrap()
                .effective_reviews
                .len(),
            1
        );
        match case {
            "pat-revoke" => {
                f.core
                    .revoke_personal_access_token("reviewer", pat.token.id)
                    .unwrap();
            }
            "session-revoke" => {
                f.core.revoke_session(&session.token).unwrap();
            }
            "pat-expire" | "session-expire" => {
                f.core
                    .with_global_mutation(|| {
                        let mut state = f.core.runtime.state.write();
                        let previous = state.clone();
                        if case == "pat-expire" {
                            state
                                .personal_tokens
                                .get_mut(&pat.token.id)
                                .unwrap()
                                .expires_at = Some(Utc::now() - Duration::seconds(1));
                        } else {
                            state
                                .sessions
                                .values_mut()
                                .find(|stored| stored.id == session.session.id)
                                .unwrap()
                                .expires_at = Utc::now() - Duration::seconds(1);
                        }
                        f.core.persist_after_mutation(&mut state, previous)
                    })
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(f.core.get_account("reviewer").unwrap().auth_epoch, epoch);
        assert!(
            f.core
                .review_qualification("owner", "demo", f.pr.number)
                .unwrap()
                .effective_reviews
                .is_empty(),
            "case {case}"
        );
        let repo_id = f.repo.id;
        let number = f.pr.number;
        let Fixture {
            core,
            observer,
            database,
            _directory,
            ..
        } = f;
        drop(core);
        let reopened = ForgeCore::open_managed(&database, &observer.root)
            .unwrap()
            .with_review_git_observer(observer)
            .unwrap();
        assert!(
            reopened
                .review_qualification("owner", "demo", number)
                .unwrap()
                .effective_reviews
                .is_empty(),
            "restart case {case}"
        );
        let (_, reader) = session_actor(&reopened, "second");
        let history = reopened
            .bound_review_history(&reader, repo_id, number)
            .unwrap();
        assert_eq!(history.events, vec![event]);
        drop(reopened);
        drop(_directory);
    }
}

#[test]
fn credential_revocation_keeps_rejection_without_resurrecting_prior_approval() {
    let f = fixture();
    let (_, actor) = session_actor(&f.core, "reviewer");
    let first = f
        .core
        .create_personal_access_token(&actor, "first", None)
        .unwrap();
    let second = f
        .core
        .create_personal_access_token(&actor, "second", None)
        .unwrap();
    for (secret, verdict) in [
        (&first.secret, ReviewState::Approved),
        (&second.secret, ReviewState::ChangesRequested),
    ] {
        let reviewer = f
            .core
            .authenticate_actor(ActorCredential::PersonalAccessToken(secret))
            .unwrap();
        let challenge = f
            .core
            .create_review_challenge(&reviewer, f.repo.id, f.pr.number, HEAD)
            .unwrap();
        f.core
            .submit_bound_review(
                &reviewer,
                f.repo.id,
                f.pr.number,
                request(&challenge, verdict),
            )
            .unwrap();
    }
    f.core
        .revoke_personal_access_token("reviewer", second.token.id)
        .unwrap();
    let qualification = f
        .core
        .review_qualification("owner", "demo", f.pr.number)
        .unwrap();
    assert_eq!(qualification.effective_reviews.len(), 1);
    assert_eq!(
        qualification.effective_reviews[0].state,
        ReviewState::ChangesRequested
    );
    assert_eq!(
        f.core
            .bound_review_history(&actor, f.repo.id, f.pr.number)
            .unwrap()
            .events
            .len(),
        2
    );
}
