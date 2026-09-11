use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

use crate::*;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn repository(core: &ForgeCore, owner: &str) -> Repository {
    core.create_repository(
        owner,
        CreateRepositoryRequest {
            name: "demo".into(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn check(core: &ForgeCore, owner: &str) -> Result<CheckRun> {
    core.create_check_run(
        owner,
        "demo",
        CreateCheckRunRequest {
            name: "demo/required".into(),
            head_sha: HEAD.into(),
            ..Default::default()
        },
    )
}

fn await_admission(core: &ForgeCore, previous: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.coordinator().admission_entries() == previous {
        assert!(
            Instant::now() < deadline,
            "public operation did not reach admission"
        );
        std::thread::yield_now();
    }
}

#[test]
fn public_check_writer_waits_for_its_repository_but_another_repository_progresses() {
    let core = ForgeCore::new();
    let first = repository(&core, "first");
    repository(&core, "second");
    let (sender, receiver) = mpsc::channel();
    let worker = core.clone();
    let mut handle = None;
    core.coordinator()
        .with_repositories(&[first.id], || {
            let before = core.coordinator().admission_entries();
            handle = Some(std::thread::spawn(move || {
                sender.send(check(&worker, "first")).unwrap()
            }));
            await_admission(&core, before);
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            let second = core.clone();
            std::thread::spawn(move || check(&second, "second"))
                .join()
                .unwrap()?;
            assert_eq!(
                core.list_check_runs("second", "demo", Some(HEAD))?
                    .total_count,
                1
            );
            Ok(())
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    handle.unwrap().join().unwrap();
    assert_eq!(
        core.list_check_runs("first", "demo", Some(HEAD))
            .unwrap()
            .total_count,
        1
    );
}

#[test]
fn public_review_and_policy_mutations_wait_for_the_repository_guard() {
    let core = ForgeCore::new();
    let repo = repository(&core, "alice");
    core.ensure_user("reviewer").unwrap();
    let pr = core
        .create_pull_request(
            "alice",
            "demo",
            "alice",
            CreatePullRequestRequest {
                title: "change".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(HEAD.into()),
                ..Default::default()
            },
        )
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::new();
    core.coordinator()
        .with_repositories(&[repo.id], || {
            let before = core.coordinator().admission_entries();
            let (worker, review_sender) = (core.clone(), sender.clone());
            handles.push(std::thread::spawn(move || {
                review_sender
                    .send(
                        worker
                            .create_review(
                                "alice",
                                "demo",
                                pr.number,
                                "reviewer",
                                CreateReviewRequest {
                                    event: ReviewState::Approved,
                                    expected_head_sha: Some(HEAD.into()),
                                    body: None,
                                    comments: Vec::new(),
                                },
                            )
                            .map(|_| ()),
                    )
                    .unwrap()
            }));
            await_admission(&core, before);
            let before = core.coordinator().admission_entries();
            let (worker, policy_sender) = (core.clone(), sender.clone());
            handles.push(std::thread::spawn(move || {
                policy_sender
                    .send(
                        worker
                            .set_branch_protection(
                                "alice",
                                "demo",
                                "main",
                                SetBranchProtectionRequest {
                                    required_approving_review_count: 1,
                                    ..Default::default()
                                },
                            )
                            .map(|_| ()),
                    )
                    .unwrap()
            }));
            await_admission(&core, before);
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            assert!(core.list_reviews("alice", "demo", pr.number)?.is_empty());
            Ok(())
        })
        .unwrap();
    for _ in 0..2 {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        core.list_reviews("alice", "demo", pr.number).unwrap().len(),
        1
    );
    assert_eq!(
        core.get_branch_protection("alice", "demo", "main")
            .unwrap()
            .required_approving_review_count,
        1
    );
}

#[test]
fn credential_revocation_waits_for_an_active_repository_operation() {
    let core = ForgeCore::new();
    let repo = repository(&core, "alice");
    core.create_account("alice", "correct horse battery", UserRole::Admin)
        .unwrap();
    let session = core
        .create_session("alice", "correct horse battery")
        .unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let pat = core
        .create_personal_access_token(&actor, "test", None)
        .unwrap();
    let (sender, receiver) = mpsc::channel();
    let worker = core.clone();
    let mut handle = None;
    core.coordinator()
        .with_repositories(&[repo.id], || {
            let before = core.coordinator().admission_entries();
            handle = Some(std::thread::spawn(move || {
                sender
                    .send(worker.revoke_personal_access_token("alice", pat.token.id))
                    .unwrap()
            }));
            await_admission(&core, before);
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            assert!(
                core.authenticate_personal_access_token(&pat.secret)
                    .is_some()
            );
            Ok(())
        })
        .unwrap();
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()
    );
    handle.unwrap().join().unwrap();
    assert!(
        core.authenticate_personal_access_token(&pat.secret)
            .is_none()
    );
}

#[test]
fn checked_grant_revalidates_administrator_access_after_waiting_for_authority() {
    let core = ForgeCore::new();
    repository(&core, "team");
    core.create_account("operator", "correct horse battery", UserRole::User)
        .unwrap();
    core.create_account("recipient", "correct horse battery", UserRole::User)
        .unwrap();
    core.grant_repo_access(
        "maintenance",
        "operator",
        "team",
        "demo",
        RepoAccessLevel::Admin,
    )
    .unwrap();
    let (sender, receiver) = mpsc::channel();
    let worker = core.clone();
    let mut handle = None;
    core.coordinator()
        .with_authority(&[], || {
            let before = core.coordinator().admission_entries();
            handle = Some(std::thread::spawn(move || {
                sender
                    .send(worker.grant_repo_access_checked(
                        "operator",
                        "recipient",
                        "team",
                        "demo",
                        RepoAccessLevel::Write,
                    ))
                    .unwrap()
            }));
            await_admission(&core, before);
            // Simulate the prior admitted revocation while owning the authority
            // gate. The waiting public wrapper must not use its earlier role.
            core.runtime.state.write().repo_grants.remove(&(
                "operator".into(),
                "team".into(),
                "demo".into(),
            ));
            Ok(())
        })
        .unwrap();
    let result = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    handle.unwrap().join().unwrap();
    assert!(
        matches!(
            result,
            Err(ForgeError::BranchProtection(_)) | Err(ForgeError::Forbidden(_))
        ),
        "{result:?}"
    );
    assert!(core.repo_access_for("recipient", "team", "demo").is_none());
}

#[test]
fn public_mutation_rejects_a_repository_replaced_after_its_uuid_hint() {
    let core = ForgeCore::new();
    let original = repository(&core, "alice");
    let (sender, receiver) = mpsc::channel();
    let worker = core.clone();
    let mut handle = None;
    core.coordinator()
        .with_authority(&[], || {
            let before = core.coordinator().admission_entries();
            handle = Some(std::thread::spawn(move || {
                sender.send(check(&worker, "alice")).unwrap()
            }));
            await_admission(&core, before); // UUID hint was captured before this entry.
            core.runtime
                .state
                .write()
                .repos
                .get_mut(&("alice".into(), "demo".into()))
                .unwrap()
                .id = uuid::Uuid::new_v4();
            Ok(())
        })
        .unwrap();
    let result = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    handle.unwrap().join().unwrap();
    assert!(matches!(result, Err(ForgeError::Conflict(_))), "{result:?}");
    assert_ne!(
        core.get_repository("alice", "demo").unwrap().id,
        original.id
    );
    assert_eq!(
        core.list_check_runs("alice", "demo", Some(HEAD))
            .unwrap()
            .total_count,
        0
    );
}

#[test]
fn concurrent_opposite_fork_operations_complete_with_sorted_uuid_guards() {
    let core = ForgeCore::new();
    repository(&core, "alice");
    repository(&core, "bob");
    core.ensure_user("author").unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();
    let mut handles = Vec::new();
    for (destination, source) in [("alice", "bob/demo"), ("bob", "alice/demo")] {
        let (core, barrier, sender) = (core.clone(), Arc::clone(&barrier), sender.clone());
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            for index in 0..8 {
                core.create_pull_request(
                    destination,
                    "demo",
                    "author",
                    CreatePullRequestRequest {
                        title: format!("change {index}"),
                        head: format!("topic-{index}"),
                        base: "main".into(),
                        source_repository: Some(source.into()),
                        head_sha: Some(HEAD.into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            sender.send(()).unwrap();
        }));
    }
    for _ in 0..2 {
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        core.list_pull_requests("alice", "demo", None)
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        core.list_pull_requests("bob", "demo", None).unwrap().len(),
        8
    );
}
