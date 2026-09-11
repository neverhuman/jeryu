//! Real-router review history and self-dismissal authorization regressions.

use super::*;
use axum::body::Body;
use axum::http::Request;
use jeryu_core::{MergeBlocker, Review};
use tower::ServiceExt;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const NEXT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Fixture {
    app: AxumRouter,
    core: ForgeCore,
    repo_id: String,
    number: u64,
    tokens: BTreeMap<String, String>,
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new(durable: bool) -> Self {
        let directory = tempdir().unwrap();
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let core = if durable {
            ForgeCore::open_sqlite(directory.path().join("forge.sqlite")).unwrap()
        } else {
            ForgeCore::new()
        };
        let repo = core
            .create_repository(
                "alice",
                CreateRepositoryRequest {
                    name: "demo".to_string(),
                    private: true,
                    default_branch: Some("main".to_string()),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut tokens = BTreeMap::new();
        for login in ["admin", "bob", "carol", "reader", "mallory"] {
            core.create_account(
                login,
                "review-route-fixture-password",
                if login == "admin" {
                    UserRole::Admin
                } else {
                    UserRole::User
                },
            )
            .unwrap();
            tokens.insert(
                login.to_string(),
                core.create_personal_access_token(
                    &super::credential_actor(&core, login, "review-route-fixture-password"),
                    "route fixture",
                    None,
                )
                .unwrap()
                .secret,
            );
        }
        for (login, access) in [
            ("bob", RepoAccessLevel::Write),
            ("carol", RepoAccessLevel::Write),
            ("reader", RepoAccessLevel::Read),
        ] {
            core.grant_repo_access("admin", login, "alice", "demo", access)
                .unwrap();
        }
        core.set_branch_protection(
            "alice",
            "demo",
            "main",
            SetBranchProtectionRequest {
                required_approving_review_count: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let number = open_pull(&core, "demo", "feature");
        let app = app(
            WebState::new_with_git_storage(core.clone(), directory.path().join("git"))
                .with_auth(true, false, false),
            &directory.path().join("no-spa"),
        );
        Self {
            app,
            core,
            repo_id: repo.id.to_string(),
            number,
            tokens,
            directory,
        }
    }

    fn history_path(&self) -> String {
        format!(
            "/api/v1/repos/{}/pulls/{}/reviews",
            self.repo_id, self.number
        )
    }

    fn dismissal_path(&self, target: impl std::fmt::Display) -> String {
        format!("{}/{target}/dismiss", self.history_path())
    }

    fn submit(&self, event: ReviewState) -> Review {
        self.core
            .create_review(
                "alice",
                "demo",
                self.number,
                "bob",
                CreateReviewRequest {
                    event,
                    body: Some("review evidence".to_string()),
                    comments: Vec::new(),
                    expected_head_sha: Some(HEAD.to_string()),
                },
            )
            .unwrap()
    }

    async fn request(
        &self,
        method: HttpMethod,
        path: &str,
        actor: Option<&str>,
        body: Value,
    ) -> AxumResponse {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(actor) = actor {
            request = request.header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.tokens[actor]),
            );
        }
        self.app
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap()
    }

    fn snapshot(&self) -> Value {
        let profiles = [
            "alice", "admin", "bob", "carol", "reader", "mallory", "spoofed",
        ]
        .map(|login| self.core.get_user(login).ok());
        json!({
            "pull": self.core.get_pull_request("alice", "demo", self.number).unwrap(),
            "reviews": self.core.list_reviews("alice", "demo", self.number).unwrap(),
            "comments": self.core.list_review_comments("alice", "demo", self.number).unwrap(),
            "accounts": self.core.list_accounts(),
            "profiles": profiles,
        })
    }

    async fn assert_refused(
        &self,
        path: &str,
        actor: Option<&str>,
        body: Value,
        status: StatusCode,
    ) {
        let before = self.snapshot();
        let response = self.request(HttpMethod::POST, path, actor, body).await;
        assert_eq!(response.status(), status, "{path}");
        let payload = response_json(response).await;
        assert!(payload["code"].as_str().is_some(), "{payload}");
        if payload["code"] == "forbidden" {
            assert_eq!(payload["error"]["code"], "forbidden");
            assert_eq!(payload["purpose"], "dismiss pull request review");
            assert_eq!(payload["docs_url"], "docs/errors.md");
            assert!(!payload["common_fixes"].as_array().unwrap().is_empty());
            assert!(payload["repair_hint"].as_str().is_some());
        }
        assert_eq!(
            self.snapshot(),
            before,
            "refusal changed durable review state"
        );
    }
}

fn open_pull(core: &ForgeCore, repo: &str, branch: &str) -> u64 {
    core.create_pull_request(
        "alice",
        repo,
        "alice",
        CreatePullRequestRequest {
            title: "review transport".to_string(),
            head: branch.to_string(),
            base: "main".to_string(),
            head_sha: Some(HEAD.to_string()),
            ..Default::default()
        },
    )
    .unwrap()
    .number
}

fn dismissal() -> Value {
    json!({"expected_head_sha": HEAD, "reason": "withdraw my current verdict"})
}

#[tokio::test]
async fn pulls_review_routes_require_authentication_and_repository_access() {
    let f = Fixture::new(false);
    let target = f.submit(ReviewState::Approved);
    let before = f.snapshot();
    for (actor, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("mallory"), StatusCode::FORBIDDEN),
    ] {
        let response = f
            .request(HttpMethod::GET, &f.history_path(), actor, Value::Null)
            .await;
        assert_eq!(response.status(), expected);
        f.assert_refused(&f.dismissal_path(target.id), actor, dismissal(), expected)
            .await;
    }
    let response = f
        .request(
            HttpMethod::GET,
            &f.history_path(),
            Some("reader"),
            Value::Null,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history = response_json(response).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["id"], target.id.to_string());
    assert_eq!(history[0]["effective"], true);
    for path in [
        format!("/api/v1/repos/{}/pulls/999/reviews", f.repo_id),
        "/api/v1/repos/00000000-0000-0000-0000-000000000000/pulls/1/reviews".to_string(),
    ] {
        let response = f
            .request(HttpMethod::GET, &path, Some("bob"), Value::Null)
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert_eq!(f.snapshot(), before);
    // A read-only grant must also block withdrawal of the caller's own verdict;
    // targeting Bob's verdict alone would only exercise Core's actor check.
    let reader_target = f
        .core
        .create_review(
            "alice",
            "demo",
            f.number,
            "reader",
            CreateReviewRequest {
                event: ReviewState::Approved,
                expected_head_sha: Some(HEAD.to_string()),
                body: None,
                comments: Vec::new(),
            },
        )
        .unwrap();
    f.assert_refused(
        &f.dismissal_path(reader_target.id),
        Some("reader"),
        dismissal(),
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn pulls_self_dismissal_preserves_history_and_agrees_with_merge_protection() {
    for verdict in [ReviewState::Approved, ReviewState::ChangesRequested] {
        let f = Fixture::new(false);
        f.submit(ReviewState::Approved);
        let target = f.submit(verdict);
        let comment = f.submit(ReviewState::Commented);
        let response = f
            .request(
                HttpMethod::POST,
                &f.dismissal_path(target.id),
                Some("bob"),
                dismissal(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let detail = response_json(response).await;
        assert_eq!(detail["summary"]["review"]["approvals"], 0);
        assert_eq!(detail["summary"]["review"]["changes_requested"], 0);
        assert_eq!(
            detail["summary"]["review"]["user_review_state"],
            Value::Null
        );
        assert_eq!(detail["merge_passport"]["status"], "blocked");
        let history = response_json(
            f.request(HttpMethod::GET, &f.history_path(), Some("bob"), Value::Null)
                .await,
        )
        .await;
        assert_eq!(history, detail["reviews"]);
        assert_eq!(history.as_array().unwrap().len(), 4);
        assert!(
            history
                .as_array()
                .unwrap()
                .iter()
                .all(|review| review["effective"] == false)
        );
        assert_eq!(history[2]["id"], comment.id.to_string());
        assert_eq!(history[2]["state"], "COMMENTED");
        assert_eq!(history[3]["author"], "bob");
        assert_eq!(history[3]["state"], "DISMISSED");
        assert_eq!(history[3]["dismissed_review_id"], target.id.to_string());
        assert_eq!(history[3]["head_sha"], HEAD);
        assert_eq!(history[3]["body_markdown"], "withdraw my current verdict");
        let evaluation = f
            .core
            .evaluate_pull_request("alice", "demo", f.number, Some(HEAD))
            .unwrap();
        assert!(
            evaluation
                .blockers
                .iter()
                .any(|blocker| matches!(blocker, MergeBlocker::MissingReview { approved: 0, .. }))
        );
        f.assert_refused(
            &f.dismissal_path(target.id),
            Some("bob"),
            dismissal(),
            StatusCode::CONFLICT,
        )
        .await;
        let dismissal_id = history[3]["id"].as_str().unwrap();
        f.assert_refused(
            &f.dismissal_path(dismissal_id),
            Some("bob"),
            dismissal(),
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
}

#[tokio::test]
async fn pulls_self_dismissal_never_accepts_another_actor_or_admin_override() {
    let f = Fixture::new(false);
    let target = f.submit(ReviewState::Approved);
    for actor in ["carol", "admin"] {
        let mut body = dismissal();
        body["actor"] = json!("bob");
        body["author"] = json!("bob");
        body["login"] = json!("bob");
        f.assert_refused(
            &f.dismissal_path(target.id),
            Some(actor),
            body,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    let mut body = dismissal();
    body["actor"] = json!("spoofed");
    body["review_id"] = json!(uuid::Uuid::nil());
    let response = f
        .request(
            HttpMethod::POST,
            &f.dismissal_path(target.id),
            Some("bob"),
            body,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let history = f.core.list_reviews("alice", "demo", f.number).unwrap();
    assert_eq!(history.last().unwrap().author, "bob");
    assert_eq!(history.last().unwrap().dismissed_review_id, Some(target.id));
    assert!(f.core.get_user("spoofed").is_err());
}

#[tokio::test]
async fn pulls_self_dismissal_rejects_invalid_body_head_and_target_without_mutation() {
    let f = Fixture::new(false);
    let target = f.submit(ReviewState::Approved);
    for body in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"reason": "withdraw"}),
        json!({"expected_head_sha": HEAD}),
        json!({"expected_head_sha": HEAD, "reason": 42}),
        json!({"expected_head_sha": HEAD, "reason": " \n\t"}),
        json!({"expected_head_sha": "abc", "reason": "withdraw"}),
        json!({"expected_head_sha": "A".repeat(40), "reason": "withdraw"}),
        json!({"expected_head_sha": "g".repeat(40), "reason": "withdraw"}),
    ] {
        f.assert_refused(
            &f.dismissal_path(target.id),
            Some("bob"),
            body,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    f.assert_refused(
        &f.dismissal_path("invalid-uuid"),
        Some("bob"),
        dismissal(),
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    f.assert_refused(
        &f.dismissal_path(uuid::Uuid::nil()),
        Some("bob"),
        dismissal(),
        StatusCode::NOT_FOUND,
    )
    .await;
    f.assert_refused(
        &f.dismissal_path(target.id),
        Some("bob"),
        json!({"expected_head_sha": NEXT, "reason": "withdraw"}),
        StatusCode::CONFLICT,
    )
    .await;
    f.assert_refused(&f.history_path(), Some("bob"), json!({"verdict": "dismissed", "expected_head_sha": HEAD, "body_markdown": null, "thread_comments": [], "evidence": null}), StatusCode::UNPROCESSABLE_ENTITY).await;
}

#[tokio::test]
async fn pulls_self_dismissal_binds_current_verdict_repository_pull_and_head() {
    let f = Fixture::new(false);
    let superseded = f.submit(ReviewState::Approved);
    let target = f.submit(ReviewState::ChangesRequested);
    let comment = f.submit(ReviewState::Commented);
    f.assert_refused(
        &f.dismissal_path(superseded.id),
        Some("bob"),
        dismissal(),
        StatusCode::CONFLICT,
    )
    .await;
    f.assert_refused(
        &f.dismissal_path(comment.id),
        Some("bob"),
        dismissal(),
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let other_number = open_pull(&f.core, "demo", "another-feature");
    let other_repo = f
        .core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "other".to_string(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    f.core
        .grant_repo_access("admin", "bob", "alice", "other", RepoAccessLevel::Write)
        .unwrap();
    let other_repo_number = open_pull(&f.core, "other", "feature");
    let other_repo_id = other_repo.id.to_string();
    for (repo_id, repo, number) in [
        (&f.repo_id, "demo", other_number),
        (&other_repo_id, "other", other_repo_number),
    ] {
        let before = f.core.get_pull_request("alice", repo, number).unwrap();
        f.assert_refused(
            &format!(
                "/api/v1/repos/{repo_id}/pulls/{number}/reviews/{}/dismiss",
                target.id
            ),
            Some("bob"),
            dismissal(),
            StatusCode::NOT_FOUND,
        )
        .await;
        assert_eq!(
            serde_json::to_value(f.core.get_pull_request("alice", repo, number).unwrap()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert!(
            f.core
                .list_reviews("alice", repo, number)
                .unwrap()
                .is_empty()
        );
    }
    f.core
        .refresh_pull_request_heads_for_ref("alice", "demo", "feature", NEXT)
        .unwrap();
    f.assert_refused(
        &f.dismissal_path(target.id),
        Some("bob"),
        json!({"expected_head_sha": NEXT, "reason": "withdraw"}),
        StatusCode::CONFLICT,
    )
    .await;
    let history = response_json(
        f.request(HttpMethod::GET, &f.history_path(), Some("bob"), Value::Null)
            .await,
    )
    .await;
    assert!(
        history
            .as_array()
            .unwrap()
            .iter()
            .all(|review| review["stale"] == true && review["effective"] == false)
    );
}

#[tokio::test]
async fn pulls_self_dismissal_rejects_malformed_json_and_requires_cookie_csrf() {
    let f = Fixture::new(false);
    let target = f.submit(ReviewState::Approved);
    let path = f.dismissal_path(target.id);
    let before = f.snapshot();
    for body in [Vec::new(), b"{".to_vec(), vec![0xff]] {
        let response = f
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri(&path)
                    .header(header::AUTHORIZATION, format!("Bearer {}", f.tokens["bob"]))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response_json(response).await;
        assert_eq!(body["code"], "pull_dismissal_invalid_request");
        assert_eq!(f.snapshot(), before);
    }
    let session = f
        .core
        .create_session("bob", "review-route-fixture-password")
        .unwrap();
    for csrf in [None, Some("incorrect-token")] {
        let mut request = Request::builder()
            .method(HttpMethod::POST)
            .uri(&path)
            .header(header::COOKIE, format!("jeryu-session={}", session.token))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(csrf) = csrf {
            request = request.header("x-jeryu-csrf", csrf);
        }
        let response = f
            .app
            .clone()
            .oneshot(request.body(Body::from(dismissal().to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response_json(response).await["code"], "csrf_required");
        assert_eq!(f.snapshot(), before);
    }
    let response = f
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri(path)
                .header(header::COOKIE, format!("jeryu-session={}", session.token))
                .header("x-jeryu-csrf", &session.session.csrf_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(dismissal().to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        f.core
            .list_reviews("alice", "demo", f.number)
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn pulls_self_dismissal_writer_loss_returns_503_and_rolls_back() {
    let f = Fixture::new(true);
    let target = f.submit(ReviewState::Approved);
    let db = f.directory.path().join("forge.sqlite");
    let held = f.directory.path().join("held.sqlite");
    let original = std::fs::read(&db).unwrap();
    std::fs::rename(&db, &held).unwrap();
    let before = f.snapshot();
    let response = f
        .request(
            HttpMethod::POST,
            &f.dismissal_path(target.id),
            Some("bob"),
            dismissal(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response_json(response).await;
    assert_eq!(body["code"], "writer_unavailable");
    assert_eq!(body["error"]["code"], "writer_unavailable");
    assert_eq!(body["purpose"], "dismiss pull request review");
    assert!(!body["common_fixes"].as_array().unwrap().is_empty());
    assert_eq!(body["docs_url"], "docs/errors.md");
    assert!(body["repair_hint"].as_str().is_some());
    assert_eq!(f.snapshot(), before);
    assert!(!db.exists());
    assert_eq!(std::fs::read(&held).unwrap(), original);
    std::fs::rename(&held, &db).unwrap();
    let response = f
        .request(
            HttpMethod::POST,
            &f.dismissal_path(target.id),
            Some("bob"),
            dismissal(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        f.core
            .list_reviews("alice", "demo", f.number)
            .unwrap()
            .len(),
        2
    );
}
