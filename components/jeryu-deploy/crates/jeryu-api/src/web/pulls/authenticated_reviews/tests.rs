use super::*;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, header};
use jeryu_core::{
    ActorCredential, CreatePullRequestRequest, CreateRepositoryRequest, CreateReviewRequest,
    ForgeCore, RepoAccessLevel, ReviewGitObservation, ReviewGitObserver, ReviewGitTarget, UserRole,
};
use jeryu_gitd::{GitdConfig, ManagedReviewGitObserver, RepoManager};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;
use tower::ServiceExt;

pub(in crate::web) struct Fixture {
    pub(in crate::web) core: ForgeCore,
    pub(in crate::web) router: Router,
    pub(in crate::web) repository: PathBuf,
    pub(in crate::web) prefix: String,
    pub(in crate::web) head: String,
    pub(in crate::web) base: String,
    pub(in crate::web) reviewer_token: String,
    pub(in crate::web) reviewer_token_id: Uuid,
    pub(in crate::web) reviewer_cookie: String,
    pub(in crate::web) reviewer_csrf: String,
    pub(in crate::web) author_token: String,
    pub(in crate::web) second_token: String,
    pub(in crate::web) _root: TempDir,
}

fn restrict_owned_fixture(path: &Path) {
    let metadata = std::fs::symlink_metadata(path).unwrap();
    assert!(!metadata.file_type().is_symlink());
    let directory = metadata.is_dir();
    assert!(directory || metadata.is_file());
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
    )
    .unwrap();
    if directory {
        for entry in std::fs::read_dir(path).unwrap() {
            restrict_owned_fixture(&entry.unwrap().path());
        }
    }
}

fn git(repository: &Path, arguments: &[&str], input: Option<&str>) -> String {
    let mut child = Command::new("/usr/bin/git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", repository)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args([
            "-c",
            "user.name=Review Test",
            "-c",
            "user.email=review@example.invalid",
        ])
        .args(arguments)
        .current_dir(repository)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fixture Git");
    let mut stdin = child.stdin.take().expect("fixture stdin");
    if let Some(input) = input {
        stdin
            .write_all(input.as_bytes())
            .expect("write fixture input");
    }
    drop(stdin);
    let output = child.wait_with_output().expect("reap fixture Git");
    assert!(
        output.status.success(),
        "fixture Git: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    restrict_owned_fixture(repository);
    String::from_utf8(output.stdout)
        .expect("Git UTF-8")
        .trim()
        .to_owned()
}

fn token(core: &ForgeCore, login: &str) -> (String, Uuid, String, String) {
    let session = core
        .create_session(login, "review-fixture-password-2026")
        .unwrap();
    let actor = core
        .authenticate_actor(ActorCredential::Session {
            token: &session.token,
            csrf_token: &session.session.csrf_token,
        })
        .unwrap();
    let token = core
        .create_personal_access_token(&actor, "review-fixture", None)
        .unwrap();
    (
        token.secret,
        token.token.id,
        format!("jeryu-session={}", session.token),
        session.session.csrf_token,
    )
}

impl Fixture {
    pub(in crate::web) fn new() -> Self {
        Self::with_observer(|observer| Arc::new(observer))
    }

    fn with_observer(
        attach: impl FnOnce(ManagedReviewGitObserver) -> Arc<dyn ReviewGitObserver>,
    ) -> Self {
        let root = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let storage = root.path().join("repos");
        let data = root.path().join("data");
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        restrict_owned_fixture(root.path());
        let core = ForgeCore::open_managed(data.join("forge.sqlite"), &storage).unwrap();
        let repo = core
            .create_repository(
                "alice",
                CreateRepositoryRequest {
                    name: "reviewed".into(),
                    private: true,
                    description: None,
                    default_branch: Some("main".into()),
                },
            )
            .unwrap();
        let repository = storage.join("alice/reviewed.git");
        std::fs::create_dir_all(&repository).unwrap();
        git(
            &repository,
            &["init", "--bare", "--initial-branch=main", "."],
            None,
        );
        let tree = git(&repository, &["mktree"], Some(""));
        let base = git(&repository, &["commit-tree", &tree, "-m", "base"], None);
        let blob = git(
            &repository,
            &["hash-object", "-w", "--stdin"],
            Some("reviewed source\n"),
        );
        let head_tree = git(
            &repository,
            &["mktree"],
            Some(&format!("100644 blob {blob}\tchange.txt\n")),
        );
        let head = git(
            &repository,
            &[
                "commit-tree",
                &head_tree,
                "-p",
                &base,
                "-m",
                "reviewed topic",
            ],
            None,
        );
        git(&repository, &["update-ref", "refs/heads/main", &base], None);
        git(
            &repository,
            &["update-ref", "refs/heads/topic", &head],
            None,
        );
        restrict_owned_fixture(root.path());
        let mut config = GitdConfig::new(&storage);
        config.git_bin = "/usr/bin/git".into();
        let manager = RepoManager::new(config);
        let observer = ManagedReviewGitObserver::new(manager.clone()).unwrap();
        let core = core.with_review_git_observer(attach(observer)).unwrap();
        core.create_account("alice", "review-fixture-password-2026", UserRole::Admin)
            .unwrap();
        for login in ["bob", "carol"] {
            core.create_account(login, "review-fixture-password-2026", UserRole::User)
                .unwrap();
            core.grant_repo_access("alice", login, "alice", "reviewed", RepoAccessLevel::Write)
                .unwrap();
        }
        core.create_pull_request(
            "alice",
            "reviewed",
            "alice",
            CreatePullRequestRequest {
                title: "Review actual topic".into(),
                head: "topic".into(),
                base: "main".into(),
                head_sha: Some(head.clone()),
                base_sha: Some(base.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let (reviewer_token, reviewer_token_id, reviewer_cookie, reviewer_csrf) =
            token(&core, "bob");
        let author_token = token(&core, "alice").0;
        let second_token = token(&core, "carol").0;
        let state = WebState::with_repo_manager(
            core.clone(),
            Arc::new(manager),
            root.path().join("no-spa"),
            data,
            crate::web::SplitCatalog::builtin(),
        )
        .with_auth(true, false, false);
        let router = crate::web::app(state, &root.path().join("no-spa"));
        Self {
            core,
            router,
            repository,
            prefix: format!("/api/v1/repos/{}/pulls/1", repo.id),
            head,
            base,
            reviewer_token,
            reviewer_token_id,
            reviewer_cookie,
            reviewer_csrf,
            author_token,
            second_token,
            _root: root,
        }
    }

    pub(in crate::web) async fn request(
        &self,
        method: Method,
        suffix: &str,
        bearer: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(format!("{}{suffix}", self.prefix))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(bearer) = bearer {
            request = request.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        response_body(response).await
    }

    pub(in crate::web) async fn challenge(&self, bearer: &str) -> Value {
        let (status, body) = self
            .request(
                Method::POST,
                "/review-challenges",
                Some(bearer),
                json!({"expected_head_sha": self.head}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body
    }

    pub(in crate::web) async fn approve(
        &self,
        bearer: &str,
        challenge: &Value,
    ) -> (StatusCode, Value) {
        self.request(
            Method::POST,
            "/approve",
            Some(bearer),
            json!({
                "expected_head_sha": self.head,
                "challenge_id": challenge["id"], "nonce": challenge["nonce"],
                "body_markdown": "Reviewed the observed source and evidence"
            }),
        )
        .await
    }
}

#[derive(Debug)]
struct HeldObserver {
    inner: ManagedReviewGitObserver,
    entered: Arc<tokio::sync::Notify>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    armed: Arc<std::sync::atomic::AtomicBool>,
}

impl ReviewGitObserver for HeldObserver {
    fn storage_root(&self) -> &Path {
        self.inner.storage_root()
    }

    fn observe(&self, target: &ReviewGitTarget) -> jeryu_core::Result<ReviewGitObservation> {
        if self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.entered.notify_one();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(3))
                .map_err(|_| {
                    ForgeError::WriterUnavailable("test observation hold expired".into())
                })?;
        }
        self.inner.observe(target)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn held_git_observation_and_token_creation_keep_http_responsive_on_one_async_worker() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (release, receiver) = std::sync::mpsc::channel();
    let f = Fixture::with_observer(|inner| {
        Arc::new(HeldObserver {
            inner,
            entered: entered.clone(),
            release: std::sync::Mutex::new(receiver),
            armed: armed.clone(),
        })
    });
    let router = f.router.clone();
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/review-challenges", f.prefix))
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.reviewer_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"expected_head_sha": f.head}).to_string()))
        .unwrap();
    armed.store(true, std::sync::atomic::Ordering::SeqCst);
    let started = std::time::Instant::now();
    let pending = tokio::spawn(async move { router.oneshot(request).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let comment_router = f.router.clone();
    let comment_request = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/comments", f.prefix))
        .header(header::AUTHORIZATION, format!("Bearer {}", f.second_token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "thread_id": null, "body_markdown": "concurrent comment", "file_path": "change.txt",
                "line": 1, "anchor_sha": f.head
            })
            .to_string(),
        ))
        .unwrap();
    let comment =
        tokio::spawn(async move { comment_router.oneshot(comment_request).await.unwrap() });
    tokio::task::yield_now().await;
    let token_request = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/tokens")
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", f.reviewer_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"name": "held-observer-token"}).to_string(),
        ))
        .unwrap();
    let mut token_request = Box::pin(f.router.clone().oneshot(token_request));
    // Poll the actual HTTP request before health: spawning it alone does not
    // prove that the token handler has had an opportunity to block this worker.
    std::future::poll_fn(|context| {
        use std::future::Future;
        assert!(token_request.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let health = f
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();
    let released = release.send(());
    let response = pending.await.unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "unrelated HTTP was blocked for {elapsed:?}"
    );
    released.expect("observer still held until explicit test release");
    let (status, body) = response_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let (status, body) = response_body(comment.await.unwrap()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = response_body(token_request.await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "held-observer-token");
    let issued_token = body["token"].as_str().expect("new token returned");
    assert_eq!(
        f.core
            .authenticate_personal_access_token(issued_token)
            .expect("new token authenticates after explicit release")
            .login,
        "bob"
    );
}

async fn response_body(response: AxumResponse) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

pub(in crate::web) async fn review_history_tracks_current_head_and_explicit_verdicts() {
    let mut f = Fixture::new();
    let old_head = f.head.clone();
    let first = f.challenge(&f.reviewer_token).await;
    assert_eq!(f.approve(&f.reviewer_token, &first).await.0, StatusCode::OK);
    let tree = git(
        &f.repository,
        &["rev-parse", &format!("{}^{{tree}}", f.head)],
        None,
    );
    let successor = git(
        &f.repository,
        &[
            "commit-tree",
            &tree,
            "-p",
            &f.head,
            "-m",
            "new reviewed head",
        ],
        None,
    );
    git(
        &f.repository,
        &["update-ref", "refs/heads/topic", &successor, &f.head],
        None,
    );
    f.core
        .refresh_pull_request_heads_for_ref("alice", "reviewed", "topic", &successor)
        .unwrap();
    f.head = successor;
    let (_, moved) = f
        .request(Method::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert_eq!(moved["summary"]["review"]["approvals"], 0);
    assert_eq!(moved["summary"]["review"]["user_review_state"], Value::Null);
    assert_eq!(moved["reviews"][0]["head_sha"], old_head);
    assert_eq!(moved["reviews"][0]["effective"], false);
    assert_eq!(moved["reviews"][0]["stale"], true);

    let challenge = f.challenge(&f.reviewer_token).await;
    let (status, requested) = f
        .request(
            Method::POST,
            "/reviews",
            Some(&f.reviewer_token),
            json!({
                "challenge_id": challenge["id"], "nonce": challenge["nonce"],
                "verdict": "request_changes", "expected_head_sha": f.head,
                "body_markdown": "fix this head", "thread_comments": [], "evidence": null
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{requested}");
    assert_eq!(requested["summary"]["review"]["changes_requested"], 1);
    assert_eq!(
        requested["summary"]["review"]["user_review_state"],
        "CHANGES_REQUESTED"
    );
    assert!(
        requested["merge_passport"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker["code"] == "passport_blocked_changes_requested")
    );

    let comment_challenge = f.challenge(&f.reviewer_token).await;
    let (status, commented) = f.request(Method::POST, "/reviews", Some(&f.reviewer_token), json!({
        "challenge_id": comment_challenge["id"], "nonce": comment_challenge["nonce"],
        "verdict": "comment", "expected_head_sha": f.head,
        "body_markdown": "a comment does not withdraw my rejection", "thread_comments": [], "evidence": null
    })).await;
    assert_eq!(status, StatusCode::OK, "{commented}");
    assert_eq!(commented["summary"]["review"]["changes_requested"], 1);
    let final_challenge = f.challenge(&f.reviewer_token).await;
    let (status, approved) = f.approve(&f.reviewer_token, &final_challenge).await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["summary"]["review"]["approvals"], 1);
    assert_eq!(approved["summary"]["review"]["changes_requested"], 0);
    assert_eq!(
        approved["summary"]["review"]["user_review_state"],
        "APPROVED"
    );
    assert_eq!(approved["reviews"].as_array().unwrap().len(), 4);
    assert_eq!(approved["reviews"][1]["state"], "CHANGES_REQUESTED");
    assert_eq!(approved["reviews"][1]["effective"], false);
    assert_eq!(approved["reviews"][1]["stale"], false);
    assert_eq!(approved["reviews"][3]["state"], "APPROVED");
    assert_eq!(approved["reviews"][3]["effective"], true);
    assert_eq!(approved["reviews"][3]["stale"], false);
    assert_eq!(approved["merge_passport"]["status"], "blocked");
}

pub(in crate::web) async fn review_comments_and_self_approval() {
    let f = Fixture::new();
    let challenge = f.challenge(&f.reviewer_token).await;
    let (status, review) = f.request(Method::POST, "/reviews", Some(&f.reviewer_token), json!({
        "challenge_id": challenge["id"], "nonce": challenge["nonce"],
        "verdict": "comment", "expected_head_sha": f.head, "body_markdown": "reviewed",
        "thread_comments": [{"thread_id": null, "body_markdown": "nit", "file_path": "change.txt",
            "line": 1, "anchor_sha": f.head}], "evidence": null
    })).await;
    assert_eq!(status, StatusCode::OK, "{review}");
    assert_eq!(review["summary"]["review"]["unresolved_threads"], 1);
    let (status, comment) = f
        .request(
            Method::POST,
            "/comments",
            Some(&f.second_token),
            json!({
                "thread_id": null, "body_markdown": "follow-up", "file_path": "change.txt",
                "line": 1, "anchor_sha": f.head
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{comment}");
    assert!(comment["threads"].as_array().unwrap().iter().any(|thread| {
        thread["comments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|comment| comment["body_markdown"] == "follow-up")
    }));
    let reviews = f.core.list_reviews("alice", "reviewed", 1).unwrap();
    for author in ["bob", "carol"] {
        assert!(
            reviews
                .iter()
                .any(|review| review.author == author && review.state == ReviewState::Commented)
        );
    }
    let own = f.challenge(&f.author_token).await;
    let (status, _) = f.request(Method::POST, "/reviews", Some(&f.author_token), json!({
        "challenge_id": own["id"], "nonce": own["nonce"], "verdict": "approve",
        "expected_head_sha": f.head, "body_markdown": "self approval", "thread_comments": [], "evidence": null
    })).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        f.approve(&f.author_token, &own).await.0,
        StatusCode::FORBIDDEN
    );
    assert!(
        !f.core
            .list_reviews("alice", "reviewed", 1)
            .unwrap()
            .iter()
            .any(|review| review.state == ReviewState::Approved)
    );
}

pub(in crate::web) async fn advisory_refresh_preserves_qualification_blockers() {
    let f = Fixture::new();
    f.core
        .set_branch_protection(
            "alice",
            "reviewed",
            "main",
            jeryu_core::SetBranchProtectionRequest {
                required_status_checks: vec!["ci/fast".into()],
                required_approving_review_count: 1,
                ..Default::default()
            },
        )
        .unwrap();
    for (name, conclusion) in [
        ("ci/fast", jeryu_core::CheckConclusion::Success),
        (
            "jeryu/autonomy",
            jeryu_core::CheckConclusion::ActionRequired,
        ),
    ] {
        f.core
            .create_check_run(
                "alice",
                "reviewed",
                jeryu_core::CreateCheckRunRequest {
                    name: name.into(),
                    head_sha: f.head.clone(),
                    conclusion: Some(conclusion),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let challenge = f.challenge(&f.reviewer_token).await;
    let (status, approved) = f.approve(&f.reviewer_token, &challenge).await;
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["summary"]["review"]["approvals"], 1);
    assert_eq!(approved["summary"]["checks"]["total"], 2);
    assert_eq!(approved["summary"]["checks"]["failing"], 1);
    assert_eq!(approved["merge_passport"]["status"], "blocked");
    assert!(
        approved["merge_passport"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker["code"] == "passport_blocked_authority")
    );
    assert_eq!(approved["head_tree_sha"].as_str().unwrap().len(), 40);
    assert_eq!(approved["base_tree_sha"].as_str().unwrap().len(), 40);
    assert_ne!(approved["head_tree_sha"], approved["base_tree_sha"]);
    let passport = approved["passport_hash"].clone();
    f.core
        .create_check_run(
            "alice",
            "reviewed",
            jeryu_core::CreateCheckRunRequest {
                name: "jeryu/autonomy".into(),
                head_sha: f.head.clone(),
                conclusion: Some(jeryu_core::CheckConclusion::Neutral),
                ..Default::default()
            },
        )
        .unwrap();
    let (_, refreshed) = f
        .request(Method::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert_eq!(refreshed["merge_passport"]["status"], "blocked");
    assert_eq!(refreshed["passport_hash"], passport);
    f.core
        .create_commit_status(
            "alice",
            "reviewed",
            &f.head,
            "ci",
            jeryu_core::CreateCommitStatusRequest {
                state: jeryu_core::CommitStatusState::Failure,
                context: "ci/fast".into(),
                description: Some("required lane regressed".into()),
                target_url: None,
            },
        )
        .unwrap();
    let (status, response) = f.request(Method::POST, "/merge", Some(&f.second_token), json!({
        "expected_head_sha": f.head, "expected_passport_hash": passport, "merge_method": "merge"
    })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["code"], "merge_passport_stale");
    assert_eq!(
        git(&f.repository, &["rev-parse", "refs/heads/main"], None),
        f.base
    );
}

#[tokio::test]
async fn real_http_review_is_authenticated_idempotent_and_historical_rows_are_advisory() {
    let f = Fixture::new();
    f.core
        .create_review(
            "alice",
            "reviewed",
            1,
            "invented-reviewer",
            CreateReviewRequest {
                body: None,
                event: ReviewState::Approved,
                comments: Vec::new(),
                expected_head_sha: Some(f.head.clone()),
            },
        )
        .unwrap();
    let (status, body) = f
        .request(Method::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["review"]["approvals"], 0);
    let (status, _) = f
        .request(
            Method::POST,
            "/approve",
            None,
            json!({"expected_head_sha": f.head}),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = f
        .request(
            Method::POST,
            "/approve",
            Some(&f.reviewer_token),
            json!({"expected_head_sha": f.head}),
        )
        .await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    let challenge = f.challenge(&f.reviewer_token).await;
    let (status, _) = f
        .request(
            Method::POST,
            "/approve",
            Some(&f.reviewer_token),
            json!({
                "challenge_id": challenge["id"], "nonce": challenge["nonce"]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(
        f.approve(&f.reviewer_token, &challenge).await.0,
        StatusCode::OK
    );
    assert_eq!(
        f.approve(&f.reviewer_token, &challenge).await.0,
        StatusCode::OK
    );
    let (status, history) = f
        .request(
            Method::GET,
            "/reviews",
            Some(&f.reviewer_token),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    let events = history["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        history["qualification"]["effective_reviews"][0]["author"],
        "bob"
    );
    assert_eq!(
        history["qualification"]["advisory_reviews"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(history["qualification"]["merge_qualified"], false);
    let (_, detail) = f
        .request(Method::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert_eq!(detail["summary"]["review"]["approvals"], 1);
    assert_eq!(detail["summary"]["mergeable"]["can_merge"], false);
    assert_eq!(detail["merge_passport"]["status"], "blocked");
}

#[tokio::test]
async fn real_http_rejects_self_approval_live_head_change_and_revoked_credential() {
    let f = Fixture::new();
    let author_challenge = f.challenge(&f.author_token).await;
    assert_eq!(
        f.approve(&f.author_token, &author_challenge).await.0,
        StatusCode::FORBIDDEN
    );
    let challenge = f.challenge(&f.reviewer_token).await;
    let tree = git(
        &f.repository,
        &["rev-parse", &format!("{}^{{tree}}", f.head)],
        None,
    );
    let successor = git(
        &f.repository,
        &[
            "commit-tree",
            &tree,
            "-p",
            &f.head,
            "-m",
            "changed live head",
        ],
        None,
    );
    git(
        &f.repository,
        &["update-ref", "refs/heads/topic", &successor, &f.head],
        None,
    );
    assert_eq!(
        f.approve(&f.reviewer_token, &challenge).await.0,
        StatusCode::CONFLICT
    );
    assert!(
        f.core
            .revoke_personal_access_token("bob", f.reviewer_token_id)
            .unwrap()
    );
    assert_eq!(
        f.approve(&f.reviewer_token, &challenge).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (_, history) = f
        .request(Method::GET, "/reviews", Some(&f.second_token), Value::Null)
        .await;
    assert!(history["events"].as_array().unwrap().is_empty());
    assert_eq!(
        git(&f.repository, &["rev-parse", "refs/heads/main"], None),
        f.base
    );
}

#[tokio::test]
async fn real_http_dismissal_is_targeted_and_does_not_resurrect_prior_approval() {
    let f = Fixture::new();
    let first = f.challenge(&f.reviewer_token).await;
    assert_eq!(f.approve(&f.reviewer_token, &first).await.0, StatusCode::OK);
    let second = f.challenge(&f.reviewer_token).await;
    assert_eq!(
        f.approve(&f.reviewer_token, &second).await.0,
        StatusCode::OK
    );
    let (_, history) = f
        .request(
            Method::GET,
            "/reviews",
            Some(&f.reviewer_token),
            Value::Null,
        )
        .await;
    let target = history["qualification"]["effective_reviews"][0]["id"].clone();
    let foreign = f.challenge(&f.second_token).await;
    let (status, _) = f.request(Method::POST, "/reviews/dismiss", Some(&f.second_token), json!({
        "expected_head_sha": f.head,
        "challenge_id": foreign["id"], "nonce": foreign["nonce"], "review_id": target, "reason": "withdraw"
    })).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let dismissal = f.challenge(&f.reviewer_token).await;
    let (status, body) = f.request(Method::POST, "/reviews/dismiss", Some(&f.reviewer_token), json!({
        "expected_head_sha": f.head,
        "challenge_id": dismissal["id"], "nonce": dismissal["nonce"], "review_id": target, "reason": "withdraw"
    })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, history) = f
        .request(
            Method::GET,
            "/reviews",
            Some(&f.reviewer_token),
            Value::Null,
        )
        .await;
    assert_eq!(history["events"].as_array().unwrap().len(), 3);
    assert!(
        history["qualification"]["effective_reviews"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (status, detail) = f
        .request(Method::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    let dismissed = detail["reviews"]
        .as_array()
        .expect("PR review history")
        .iter()
        .find(|review| review["state"] == "DISMISSED")
        .expect("targeted dismissal in PR detail");
    assert_eq!(dismissed["dismissed_review_id"], target);
}

#[tokio::test]
async fn real_http_session_history_needs_no_csrf_but_mutations_do() {
    let f = Fixture::new();
    let response = f
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{}/reviews", f.prefix))
                .header(header::COOKIE, &f.reviewer_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    for csrf in [None, Some("wrong-csrf"), Some(f.reviewer_csrf.as_str())] {
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(format!("{}/review-challenges", f.prefix))
            .header(header::COOKIE, &f.reviewer_cookie)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(csrf) = csrf {
            request = request.header("x-jeryu-csrf", csrf);
        }
        let response = f
            .router
            .clone()
            .oneshot(
                request
                    .body(Body::from(json!({"expected_head_sha": f.head}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if csrf == Some(f.reviewer_csrf.as_str()) {
                StatusCode::CREATED
            } else {
                StatusCode::FORBIDDEN
            }
        );
    }
    let response = f
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("{}/reviews", f.prefix))
                .header(header::COOKIE, &f.reviewer_cookie)
                .header(header::AUTHORIZATION, "Bearer rejected-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn real_http_legacy_merge_cannot_advance_git_without_the_qualified_executor() {
    let f = Fixture::new();
    let challenge = f.challenge(&f.reviewer_token).await;
    assert_eq!(
        f.approve(&f.reviewer_token, &challenge).await.0,
        StatusCode::OK
    );
    let response = f
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/repos/alice/reviewed/pulls/1/merge")
                .header(header::AUTHORIZATION, format!("Bearer {}", f.second_token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"sha": f.head, "merge_method": "merge"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        git(&f.repository, &["rev-parse", "refs/heads/main"], None),
        f.base
    );
    assert!(
        !f.core
            .get_pull_request("alice", "reviewed", 1)
            .unwrap()
            .merged
    );
}
