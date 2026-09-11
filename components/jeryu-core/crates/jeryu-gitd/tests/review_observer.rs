#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use jeryu_core::*;
use jeryu_gitd::{GitdConfig, ManagedReviewGitObserver, RepoManager};
use uuid::Uuid;

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    database: PathBuf,
    observer: Arc<ManagedReviewGitObserver>,
    target: ReviewGitTarget,
    base: String,
    head: String,
    tree: String,
}

fn git(path: &Path, arguments: &[&str], input: &[u8]) -> String {
    let mut child = Command::new("/usr/bin/git")
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .arg("--git-dir")
        .arg(path)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn private_tree(path: &Path) {
    let metadata = fs::symlink_metadata(path).unwrap();
    assert!(!metadata.file_type().is_symlink());
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if metadata.is_dir() { 0o700 } else { 0o600 }),
    )
    .unwrap();
    if metadata.is_dir() {
        for child in fs::read_dir(path).unwrap() {
            private_tree(&child.unwrap().path());
        }
    }
}

fn fixture() -> Fixture {
    assert!(
        Path::new("/usr/bin/git").is_file(),
        "real installed Git is required"
    );
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let root = directory.path().join("repos");
    let repo = root.join("owner/demo.git");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "--bare"], &[]);
    let tree = git(&repo, &["mktree"], &[]);
    let base = git(&repo, &["commit-tree", &tree, "-m", "base"], &[]);
    let head = git(
        &repo,
        &["commit-tree", &tree, "-p", &base, "-m", "change"],
        &[],
    );
    git(&repo, &["update-ref", "refs/heads/main", &base], &[]);
    git(&repo, &["update-ref", "refs/heads/topic", &head], &[]);
    private_tree(&root);
    let mut config = GitdConfig::new(&root);
    config.git_bin = "/usr/bin/git".into();
    let observer = Arc::new(ManagedReviewGitObserver::new(RepoManager::new(config)).unwrap());
    let repository = ReviewGitRepository {
        id: Uuid::new_v4(),
        owner: "owner".into(),
        name: "demo".into(),
    };
    let target = ReviewGitTarget {
        source: repository.clone(),
        destination: repository,
        source_ref: "refs/heads/topic".into(),
        destination_ref: "refs/heads/main".into(),
    };
    let database = directory.path().join("forge.sqlite");
    Fixture {
        _directory: directory,
        root,
        repo,
        database,
        observer,
        target,
        base,
        head,
        tree,
    }
}

#[test]
fn real_direct_refs_and_trees_produce_bound_authenticated_review_and_detect_movement() {
    let f = fixture();
    let observed = f.observer.observe(&f.target).unwrap();
    assert_eq!(observed.source.commit_sha, f.head);
    assert_eq!(observed.destination.commit_sha, f.base);
    assert_eq!(observed.source.tree_sha, f.tree);
    assert_eq!(observed.destination.tree_sha, f.tree);
    assert!(observed.source.identity.repository_inode > 0);
    assert_eq!(observed.source.identity.git_executable_sha256.len(), 64);
    let core = ForgeCore::open_managed(&f.database, &f.root)
        .unwrap()
        .with_review_git_observer(f.observer.clone())
        .unwrap();
    let repository = core
        .create_repository(
            "owner",
            CreateRepositoryRequest {
                name: "demo".into(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    core.create_account("reviewer", "correct horse battery", UserRole::User)
        .unwrap();
    core.grant_repo_access(
        "operator",
        "reviewer",
        "owner",
        "demo",
        RepoAccessLevel::Write,
    )
    .unwrap();
    let pr = core
        .create_pull_request(
            "owner",
            "demo",
            "author",
            CreatePullRequestRequest {
                title: "change".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(f.head.clone()),
                base_sha: Some(f.base.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let session = core
        .create_session("reviewer", "correct horse battery")
        .unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let challenge = core
        .create_review_challenge(&actor, repository.id, pr.number, &f.head)
        .unwrap();
    let request = SubmitBoundReviewRequest {
        challenge_id: challenge.id,
        nonce: challenge.nonce,
        expected_head_sha: f.head.clone(),
        event: ReviewState::Approved,
        body: None,
        comments: vec![],
    };
    let accepted = core
        .submit_bound_review(&actor, repository.id, pr.number, request)
        .unwrap();
    assert_eq!(accepted.snapshot.git, observed);
    assert_eq!(
        core.review_qualification("owner", "demo", pr.number)
            .unwrap()
            .effective_reviews,
        vec![accepted.review]
    );
    let pending = core
        .create_review_challenge(&actor, repository.id, pr.number, &f.head)
        .unwrap();
    git(
        &f.repo,
        &["update-ref", "refs/heads/topic", &f.base, &f.head],
        &[],
    );
    private_tree(&f.root);
    let error = core
        .submit_bound_review(
            &actor,
            repository.id,
            pr.number,
            SubmitBoundReviewRequest {
                challenge_id: pending.id,
                nonce: pending.nonce,
                expected_head_sha: f.head,
                event: ReviewState::Approved,
                body: None,
                comments: vec![],
            },
        )
        .unwrap_err();
    assert!(matches!(error, ForgeError::Conflict(_)));
    assert_eq!(
        core.bound_review_history(&actor, repository.id, pr.number)
            .unwrap()
            .events
            .len(),
        1
    );
    assert!(matches!(
        core.evaluate_merge_readiness("owner", "demo", pr.number, None),
        Err(ForgeError::WriterUnavailable(_))
    ));
    assert_eq!(git(&f.repo, &["rev-parse", "refs/heads/main"], &[]), f.base);
}

#[test]
fn missing_ref_and_corrupt_git_are_distinct_fail_closed_outcomes() {
    let f = fixture();
    let mut missing = f.target.clone();
    missing.source_ref = "refs/heads/missing".into();
    assert!(matches!(
        f.observer.observe(&missing),
        Err(ForgeError::NotFound(_))
    ));
    fs::write(f.repo.join("refs/heads/topic"), "invalid object id\n").unwrap();
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn symbolic_refs_and_alternate_storage_cannot_supply_review_evidence() {
    let f = fixture();
    git(
        &f.repo,
        &["symbolic-ref", "refs/heads/topic", "refs/heads/main"],
        &[],
    );
    private_tree(&f.root);
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
    git(
        &f.repo,
        &["symbolic-ref", "--delete", "refs/heads/topic"],
        &[],
    );
    git(&f.repo, &["update-ref", "refs/heads/topic", &f.head], &[]);
    private_tree(&f.root);
    fs::write(
        f.repo.join("objects/info/alternates"),
        "/unapproved/object-store\n",
    )
    .unwrap();
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn symlinked_repository_and_replaced_root_are_rejected() {
    let f = fixture();
    let retained = f.root.join("owner/retained.git");
    fs::rename(&f.repo, &retained).unwrap();
    symlink(&retained, &f.repo).unwrap();
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
    fs::remove_file(&f.repo).unwrap();
    fs::rename(&retained, &f.repo).unwrap();
    let saved = f._directory.path().join("retained-repos");
    fs::rename(&f.root, &saved).unwrap();
    fs::create_dir(&f.root).unwrap();
    private_tree(&f.root);
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn relative_executables_and_repository_config_includes_are_not_admitted() {
    let f = fixture();
    let mut config = GitdConfig::new(&f.root);
    config.git_bin = "git".into();
    assert!(ManagedReviewGitObserver::new(RepoManager::new(config)).is_err());
    git(
        &f.repo,
        &["config", "include.path", "/unapproved/config"],
        &[],
    );
    private_tree(&f.root);
    assert!(matches!(
        f.observer.observe(&f.target),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn ambient_git_routing_does_not_change_managed_observation() {
    if std::env::var_os("JERYU_REVIEW_OBSERVER_ENV_CHILD").is_some() {
        let f = fixture();
        assert_eq!(
            f.observer.observe(&f.target).unwrap().source.commit_sha,
            f.head
        );
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ambient_git_routing_does_not_change_managed_observation",
            "--nocapture",
        ])
        .env("JERYU_REVIEW_OBSERVER_ENV_CHILD", "1")
        .env("GIT_DIR", "/wrong/repository")
        .env("GIT_OBJECT_DIRECTORY", "/wrong/objects")
        .env("GIT_COMMON_DIR", "/wrong/common")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "include.path")
        .env("GIT_CONFIG_VALUE_0", "/wrong/config")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
