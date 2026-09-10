mod common;

use jeryu_gitd::refs::{RefService, ZERO_OID, validate_ref_name};
use jeryu_gitd::repo::Repository;
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::path::PathBuf;
use std::process::Command;

#[test]
fn refs_lists_created_branch() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-refs-list");
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|err| panic!("refs failed: {err}"));
    assert!(refs.iter().any(|r| r.name == "refs/heads/main"));
    fixture.cleanup();
}

#[test]
fn ref_service_denies_protected_main_delete_before_mutation() {
    if !common::git_available() {
        return;
    }
    let fixture = seed_bare_with_main("jeryu-refs-protected-delete");
    let err = RefService::new(fixture.manager.clone())
        .update_ref(
            &fixture.repo,
            "alice",
            "refs/heads/main",
            ZERO_OID,
            Some(&fixture.main_oid),
        )
        .unwrap_err();

    assert!(err.to_string().contains("cannot delete"));
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|read_err| panic!("refs failed after denied delete: {read_err}"));
    assert!(
        refs.iter()
            .any(|r| r.name == "refs/heads/main" && r.oid == fixture.main_oid)
    );
    fixture.cleanup();
}

#[test]
fn ref_service_denies_protected_main_non_fast_forward_before_mutation() {
    if !common::git_available() {
        return;
    }
    let mut fixture = seed_bare_with_main("jeryu-refs-protected-force");
    let first_oid = fixture.main_oid.clone();
    fixture.main_oid = commit_and_push(&fixture.work, &fixture.repo, "second\n");
    let second_oid = fixture.main_oid.clone();

    let err = RefService::new(fixture.manager.clone())
        .update_ref(
            &fixture.repo,
            "alice",
            "refs/heads/main",
            &first_oid,
            Some(&second_oid),
        )
        .unwrap_err();

    assert!(err.to_string().contains("cannot force-update"));
    let refs = RefService::new(fixture.manager.clone())
        .list_refs(&fixture.repo)
        .unwrap_or_else(|read_err| panic!("refs failed after denied update: {read_err}"));
    assert!(
        refs.iter()
            .any(|r| r.name == "refs/heads/main" && r.oid == second_oid)
    );
    fixture.cleanup();
}

#[test]
fn ref_service_create_requires_absence_even_for_identical_existing_value() {
    assert!(
        common::git_available(),
        "real Git is required for ref CAS tests"
    );
    let fixture = seed_bare_with_main("jeryu-refs-create-collision");
    let initial = fixture.main_oid.clone();
    let advanced = commit_and_push(&fixture.work, &fixture.repo, "advanced\n");
    let service = RefService::new(fixture.manager.clone());
    service
        .update_ref(&fixture.repo, "alice", "refs/tags/release", &advanced, None)
        .expect("create an absent tag");

    for name in ["refs/heads/main", "refs/tags/release"] {
        for proposed in [&advanced, &initial] {
            service
                .update_ref(&fixture.repo, "alice", name, proposed, None)
                .expect_err("a create must not adopt or replace an existing ref");
            assert!(
                service
                    .list_refs(&fixture.repo)
                    .unwrap()
                    .iter()
                    .any(|reference| reference.name == name && reference.oid == advanced)
            );
        }
    }
    fixture.cleanup();
}

#[test]
fn ref_service_concurrent_creators_have_one_winner() {
    assert!(
        common::git_available(),
        "real Git is required for ref CAS tests"
    );
    let fixture = seed_bare_with_main("jeryu-refs-create-race");
    let initial = fixture.main_oid.clone();
    let advanced = commit_and_push(&fixture.work, &fixture.repo, "advanced\n");
    let service = RefService::new(fixture.manager.clone());
    let barrier = std::sync::Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = [&initial, &advanced]
            .into_iter()
            .map(|oid| {
                let service = &service;
                let repo = &fixture.repo;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    service
                        .update_ref(repo, "alice", "refs/heads/new", oid, None)
                        .map(|()| oid.clone())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    let winners: Vec<_> = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one create may succeed: {results:?}"
    );
    assert!(
        service
            .list_refs(&fixture.repo)
            .unwrap()
            .iter()
            .any(|reference| reference.name == "refs/heads/new" && &reference.oid == winners[0])
    );
    fixture.cleanup();
}

#[test]
fn ref_service_updates_and_deletes_require_the_exact_predecessor() {
    assert!(
        common::git_available(),
        "real Git is required for ref CAS tests"
    );
    let fixture = seed_bare_with_main("jeryu-refs-exact-predecessor");
    let initial = fixture.main_oid.clone();
    let advanced = commit_and_push(&fixture.work, &fixture.repo, "advanced\n");
    let service = RefService::new(fixture.manager.clone());
    let name = "refs/heads/topic";
    service
        .update_ref(&fixture.repo, "alice", name, &initial, None)
        .unwrap();
    service
        .update_ref(&fixture.repo, "alice", name, &advanced, Some(&initial))
        .expect("advance the exact predecessor");
    service
        .update_ref(&fixture.repo, "alice", name, &initial, Some(&initial))
        .expect_err("a stale predecessor must not overwrite the current head");
    for expected in [None, Some(initial.as_str())] {
        service
            .update_ref(&fixture.repo, "alice", name, ZERO_OID, expected)
            .expect_err("deletion needs the current predecessor");
    }
    assert!(
        service
            .list_refs(&fixture.repo)
            .unwrap()
            .iter()
            .any(|reference| { reference.name == name && reference.oid == advanced })
    );
    service
        .update_ref(&fixture.repo, "alice", name, ZERO_OID, Some(&advanced))
        .expect("delete the exact predecessor");
    assert!(
        !service
            .list_refs(&fixture.repo)
            .unwrap()
            .iter()
            .any(|reference| reference.name == name)
    );
    fixture.cleanup();
}

#[test]
fn ref_name_validation_rejects_command_like_and_nul_names() {
    if !common::git_available() {
        return;
    }
    assert!(validate_ref_name("-refs/heads/main").is_err());
    assert!(validate_ref_name("refs/heads/main\0shadow").is_err());
}

#[derive(Debug)]
struct BareFixture {
    root: PathBuf,
    work: PathBuf,
    manager: RepoManager,
    repo: Repository,
    main_oid: String,
}

impl BareFixture {
    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(self.root);
        let _ = std::fs::remove_dir_all(self.work);
    }
}

fn seed_bare_with_main(prefix: &str) -> BareFixture {
    let root = common::temp_dir(&format!("{prefix}-root"));
    let work = common::temp_dir(&format!("{prefix}-work"));
    let manager = RepoManager::new(GitdConfig::new(&root));
    let id = RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("id failed: {err}"));
    let repo = manager
        .create_bare(&id)
        .unwrap_or_else(|err| panic!("create failed: {err}"));
    run_git(&work, &["init"], "git init");
    run_git(
        &work,
        &["config", "user.email", "test@example.invalid"],
        "git config email",
    );
    run_git(&work, &["config", "user.name", "Test"], "git config name");
    std::fs::write(work.join("README.md"), "hello\n")
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git(&work, &["add", "README.md"], "git add");
    run_git(&work, &["commit", "-m", "seed"], "git commit");
    run_git(
        &work,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "git push",
    );
    let main_oid = rev_parse_head(&work);
    BareFixture {
        root,
        work,
        manager,
        repo,
        main_oid,
    }
}

fn commit_and_push(work: &PathBuf, repo: &Repository, contents: &str) -> String {
    std::fs::write(work.join("README.md"), contents)
        .unwrap_or_else(|err| panic!("write failed: {err}"));
    run_git(work, &["add", "README.md"], "git add");
    run_git(work, &["commit", "-m", "update"], "git commit");
    run_git(
        work,
        &[
            "push",
            repo.path.to_str().unwrap_or_default(),
            "HEAD:refs/heads/main",
        ],
        "git push",
    );
    rev_parse_head(work)
}

fn rev_parse_head(work: &PathBuf) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(work)
        .output()
        .unwrap_or_else(|err| panic!("git rev-parse failed: {err}"));
    assert!(
        output.status.success(),
        "git rev-parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn run_git(work: &PathBuf, args: &[&str], label: &str) {
    let status = Command::new("git")
        .args(args)
        .current_dir(work)
        .status()
        .unwrap_or_else(|err| panic!("{label} failed to start: {err}"));
    assert!(status.success(), "{label} failed with {status}");
}
