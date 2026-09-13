//! Review transport regressions using real credentials, private SQLite and Git.

use super::*;
use crate::web::pulls::authenticated_reviews::tests::Fixture;
use axum::body::Body;
use axum::http::Request;
use jeryu_core::{MergeBlocker, Review};
use tower::ServiceExt;

fn snapshot(f: &Fixture) -> Value {
    json!({
        "pull": f.core.get_pull_request("alice", "reviewed", 1).unwrap(),
        "reviews": f.core.list_reviews("alice", "reviewed", 1).unwrap(),
        "comments": f.core.list_review_comments("alice", "reviewed", 1).unwrap(),
        "accounts": f.core.list_accounts(),
        "spoofed": f.core.get_user("spoofed").ok(),
    })
}

async fn submit(f: &Fixture, verdict: &str) -> Review {
    let challenge = f.challenge(&f.reviewer_token).await;
    let (status, body) = f.request(HttpMethod::POST, "/reviews", Some(&f.reviewer_token), json!({
        "expected_head_sha": f.head, "challenge_id": challenge["id"], "nonce": challenge["nonce"],
        "verdict": verdict, "body_markdown": "review evidence", "thread_comments": [], "evidence": null
    })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    f.core
        .list_reviews("alice", "reviewed", 1)
        .unwrap()
        .pop()
        .unwrap()
}

async fn dismissal(f: &Fixture, token: &str, target: impl std::fmt::Display) -> Value {
    let challenge = f.challenge(token).await;
    json!({"expected_head_sha": f.head, "challenge_id": challenge["id"], "nonce": challenge["nonce"],
        "review_id": target.to_string(), "reason": "withdraw my current verdict"})
}

async fn refused(f: &Fixture, suffix: &str, token: Option<&str>, body: Value, status: StatusCode) {
    let before = snapshot(f);
    let (actual, payload) = f.request(HttpMethod::POST, suffix, token, body).await;
    assert_eq!(actual, status, "{suffix}: {payload}");
    assert!(payload["code"].is_string(), "{payload}");
    if payload["error"].is_object() {
        assert_eq!(payload["error"]["code"], payload["code"]);
        assert_eq!(payload["docs_url"], "docs/errors.md");
        assert!(!payload["common_fixes"].as_array().unwrap().is_empty());
        assert!(payload["repair_hint"].is_string());
    }
    assert_eq!(snapshot(f), before, "refusal changed review state");
}

#[tokio::test]
async fn pulls_review_routes_require_authentication_and_repository_access() {
    let f = Fixture::new();
    let target = submit(&f, "approve").await;
    let body = dismissal(&f, &f.reviewer_token, target.id).await;
    let mut tokens = Vec::new();
    for login in ["reader", "mallory"] {
        f.core
            .create_account(login, "review-route-fixture-password", UserRole::User)
            .unwrap();
        tokens.push(
            f.core
                .create_personal_access_token(
                    &credential_actor(&f.core, login, "review-route-fixture-password"),
                    "route",
                    None,
                )
                .unwrap()
                .secret,
        );
    }
    f.core
        .grant_repo_access(
            "alice",
            "reader",
            "alice",
            "reviewed",
            RepoAccessLevel::Read,
        )
        .unwrap();
    for (actor, expected) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(tokens[1].as_str()), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            f.request(HttpMethod::GET, "/reviews", actor, Value::Null)
                .await
                .0,
            expected
        );
        refused(&f, "/reviews/dismiss", actor, body.clone(), expected).await;
    }
    let (status, history) = f
        .request(HttpMethod::GET, "/reviews", Some(&tokens[0]), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(history["events"].as_array().unwrap().len(), 1);
    assert_eq!(
        history["qualification"]["effective_reviews"][0]["id"],
        target.id.to_string()
    );
    refused(
        &f,
        "/reviews/dismiss",
        Some(&tokens[0]),
        body,
        StatusCode::FORBIDDEN,
    )
    .await;
    refused(
        &f,
        "/review-challenges",
        Some(&tokens[0]),
        json!({"expected_head_sha":f.head}),
        StatusCode::FORBIDDEN,
    )
    .await;
    // Revocation is checked by the same already-built router and token.
    f.core
        .revoke_repo_access_checked("alice", "reader", "alice", "reviewed")
        .unwrap();
    assert_eq!(
        f.request(HttpMethod::GET, "/reviews", Some(&tokens[0]), Value::Null)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for path in [
        f.prefix.replace("pulls/1", "pulls/999") + "/reviews",
        "/api/v1/repos/00000000-0000-0000-0000-000000000000/pulls/1/reviews".into(),
    ] {
        let response = f
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(
                        header::AUTHORIZATION,
                        format!("Bearer {}", f.reviewer_token),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn pulls_self_dismissal_preserves_history_and_agrees_with_merge_protection() {
    for verdict in ["approve", "request_changes"] {
        let f = Fixture::new();
        f.core
            .set_branch_protection(
                "alice",
                "reviewed",
                "main",
                SetBranchProtectionRequest {
                    required_approving_review_count: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        submit(&f, "approve").await;
        let target = submit(&f, verdict).await;
        let comment = submit(&f, "comment").await;
        let request = dismissal(&f, &f.reviewer_token, target.id).await;
        let (status, event) = f
            .request(
                HttpMethod::POST,
                "/reviews/dismiss",
                Some(&f.reviewer_token),
                request.clone(),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{event}");
        // A lost-return retry returns the same immutable event without appending.
        assert_eq!(
            f.request(
                HttpMethod::POST,
                "/reviews/dismiss",
                Some(&f.reviewer_token),
                request
            )
            .await,
            (StatusCode::OK, event.clone())
        );
        let (_, detail) = f
            .request(HttpMethod::GET, "", Some(&f.reviewer_token), Value::Null)
            .await;
        assert_eq!(detail["summary"]["review"]["approvals"], 0);
        assert_eq!(detail["summary"]["review"]["changes_requested"], 0);
        assert_eq!(
            detail["summary"]["review"]["user_review_state"],
            Value::Null
        );
        assert_eq!(detail["merge_passport"]["status"], "blocked");
        let history = detail["reviews"].as_array().unwrap();
        assert_eq!(history.len(), 4);
        assert!(history.iter().all(|row| row["effective"] == false));
        assert_eq!(history[2]["id"], comment.id.to_string());
        assert_eq!(history[2]["state"], "COMMENTED");
        assert_eq!(history[3]["state"], "DISMISSED");
        assert_eq!(history[3]["author"], "bob");
        assert_eq!(history[3]["dismissed_review_id"], target.id.to_string());
        assert_eq!(history[3]["head_sha"], f.head);
        assert_eq!(history[3]["body_markdown"], "withdraw my current verdict");
        let (_, bound) = f
            .request(
                HttpMethod::GET,
                "/reviews",
                Some(&f.reviewer_token),
                Value::Null,
            )
            .await;
        assert_eq!(bound["events"].as_array().unwrap().len(), 4);
        assert!(
            bound["qualification"]["effective_reviews"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let evaluation = f
            .core
            .evaluate_pull_request("alice", "reviewed", 1, Some(&f.head))
            .unwrap();
        assert!(
            evaluation
                .blockers
                .iter()
                .any(|b| matches!(b, MergeBlocker::MissingReview { approved: 0, .. }))
        );
        for id in [
            target.id.to_string(),
            event["review"]["id"].as_str().unwrap().to_string(),
        ] {
            let body = dismissal(&f, &f.reviewer_token, id).await;
            refused(
                &f,
                "/reviews/dismiss",
                Some(&f.reviewer_token),
                body,
                StatusCode::CONFLICT,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn pulls_self_dismissal_never_accepts_another_actor_or_admin_override() {
    let f = Fixture::new();
    let target = submit(&f, "approve").await;
    for token in [&f.second_token, &f.author_token] {
        let body = dismissal(&f, token, target.id).await;
        refused(
            &f,
            "/reviews/dismiss",
            Some(token),
            body.clone(),
            StatusCode::FORBIDDEN,
        )
        .await;
        for field in ["actor", "author", "login"] {
            let mut spoofed = body.clone();
            spoofed[field] = json!("bob");
            refused(
                &f,
                "/reviews/dismiss",
                Some(token),
                spoofed,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
    }
    let mut body = dismissal(&f, &f.reviewer_token, target.id).await;
    body["actor"] = json!("spoofed");
    refused(
        &f,
        "/reviews/dismiss",
        Some(&f.reviewer_token),
        body,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let body = dismissal(&f, &f.reviewer_token, target.id).await;
    assert_eq!(
        f.request(
            HttpMethod::POST,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            body
        )
        .await
        .0,
        StatusCode::OK
    );
    let history = f.core.list_reviews("alice", "reviewed", 1).unwrap();
    assert_eq!(history.last().unwrap().author, "bob");
    assert_eq!(history.last().unwrap().dismissed_review_id, Some(target.id));
    assert!(f.core.get_user("spoofed").is_err());
}

#[tokio::test]
async fn pulls_self_dismissal_rejects_invalid_body_head_and_target_without_mutation() {
    let f = Fixture::new();
    let target = submit(&f, "approve").await;
    let valid = dismissal(&f, &f.reviewer_token, target.id).await;
    for body in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"reason":"withdraw"}),
    ] {
        refused(
            &f,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            body,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    for (field, value, status) in [
        ("reason", json!(42), StatusCode::UNPROCESSABLE_ENTITY),
        ("reason", json!(" \n\t"), StatusCode::UNPROCESSABLE_ENTITY),
        (
            "review_id",
            json!("invalid-uuid"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        ("review_id", json!(uuid::Uuid::nil()), StatusCode::CONFLICT),
        (
            "expected_head_sha",
            json!("abc"),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "expected_head_sha",
            json!("A".repeat(40)),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "expected_head_sha",
            json!("g".repeat(40)),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "expected_head_sha",
            json!("b".repeat(40)),
            StatusCode::CONFLICT,
        ),
        ("nonce", json!("unissued"), StatusCode::CONFLICT),
    ] {
        let mut body = valid.clone();
        body[field] = value;
        refused(
            &f,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            body,
            status,
        )
        .await;
    }
    for field in ["expected_head_sha", "challenge_id", "nonce"] {
        let mut body = valid.clone();
        body.as_object_mut().unwrap().remove(field);
        refused(
            &f,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            body,
            StatusCode::PRECONDITION_REQUIRED,
        )
        .await;
    }
    refused(&f, "/reviews", Some(&f.reviewer_token), json!({"verdict":"dismissed", "expected_head_sha":f.head,"body_markdown":null,"thread_comments":[],"evidence":null}), StatusCode::UNPROCESSABLE_ENTITY).await;
    refused(
        &f,
        &format!("/reviews/{}/dismiss", target.id),
        Some(&f.reviewer_token),
        valid,
        StatusCode::PRECONDITION_REQUIRED,
    )
    .await;
}

#[tokio::test]
async fn pulls_self_dismissal_binds_current_verdict_repository_pull_and_head() {
    let f = Fixture::new();
    let superseded = submit(&f, "approve").await;
    let target = submit(&f, "request_changes").await;
    let comment = submit(&f, "comment").await;
    for id in [superseded.id, comment.id] {
        let body = dismissal(&f, &f.reviewer_token, id).await;
        refused(
            &f,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            body,
            StatusCode::CONFLICT,
        )
        .await;
    }
    let body = dismissal(&f, &f.reviewer_token, target.id).await;
    let repo = f
        .core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "other".into(),
                private: true,
                ..Default::default()
            },
        )
        .unwrap();
    f.core
        .grant_repo_access("alice", "bob", "alice", "other", RepoAccessLevel::Write)
        .unwrap();
    for (repo_name, prefix) in [
        ("reviewed", f.prefix.replace("pulls/1", "pulls/2")),
        ("other", format!("/api/v1/repos/{}/pulls/1", repo.id)),
    ] {
        let pr = f
            .core
            .create_pull_request(
                "alice",
                repo_name,
                "alice",
                CreatePullRequestRequest {
                    title: "other pull".into(),
                    head: "topic".into(),
                    base: "main".into(),
                    head_sha: Some(f.head.clone()),
                    base_sha: Some(f.base.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = snapshot(&f);
        let response = f
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri(format!("{prefix}/reviews/dismiss"))
                    .header(
                        header::AUTHORIZATION,
                        format!("Bearer {}", f.reviewer_token),
                    )
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(snapshot(&f), before);
        assert!(
            f.core
                .list_reviews("alice", repo_name, pr.number)
                .unwrap()
                .is_empty()
        );
    }
    f.core
        .refresh_pull_request_heads_for_ref("alice", "reviewed", "topic", &"b".repeat(40))
        .unwrap();
    let mut moved = body;
    moved["expected_head_sha"] = json!("b".repeat(40));
    refused(
        &f,
        "/reviews/dismiss",
        Some(&f.reviewer_token),
        moved,
        StatusCode::CONFLICT,
    )
    .await;
    let (_, detail) = f
        .request(HttpMethod::GET, "", Some(&f.reviewer_token), Value::Null)
        .await;
    assert!(
        detail["reviews"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["stale"] == true && r["effective"] == false)
    );
}

#[tokio::test]
async fn pulls_self_dismissal_rejects_malformed_json_and_requires_cookie_csrf() {
    let f = Fixture::new();
    let target = submit(&f, "approve").await;
    let before = snapshot(&f);
    for body in [Vec::new(), b"{".to_vec(), vec![0xff]] {
        let response = f
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(HttpMethod::POST)
                    .uri(format!("{}/reviews/dismiss", f.prefix))
                    .header(
                        header::AUTHORIZATION,
                        format!("Bearer {}", f.reviewer_token),
                    )
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response_json(response).await["code"], "invalid_input");
        assert_eq!(snapshot(&f), before);
    }
    // Bind a distinct challenge to the session credential, not the PAT.
    let response = f
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(HttpMethod::POST)
                .uri(format!("{}/review-challenges", f.prefix))
                .header(header::COOKIE, &f.reviewer_cookie)
                .header("x-jeryu-csrf", &f.reviewer_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"expected_head_sha": f.head}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let challenge = response_json(response).await;
    let body = json!({"expected_head_sha":f.head,"challenge_id":challenge["id"],"nonce":challenge["nonce"],"review_id":target.id,"reason":"withdraw"});
    for csrf in [
        None,
        Some("incorrect-token"),
        Some(f.reviewer_csrf.as_str()),
    ] {
        let mut request = Request::builder()
            .method(HttpMethod::POST)
            .uri(format!("{}/reviews/dismiss", f.prefix))
            .header(header::COOKIE, &f.reviewer_cookie)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(csrf) = csrf {
            request = request.header("x-jeryu-csrf", csrf);
        }
        let response = f
            .router
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        if csrf == Some(f.reviewer_csrf.as_str()) {
            assert_eq!(response.status(), StatusCode::OK);
        } else {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert_eq!(snapshot(&f), before);
        }
    }
    assert_eq!(
        f.core.list_reviews("alice", "reviewed", 1).unwrap().len(),
        2
    );
}

#[tokio::test]
async fn pulls_self_dismissal_writer_loss_returns_503_and_rolls_back() {
    let f = Fixture::new();
    let target = submit(&f, "approve").await;
    let request = dismissal(&f, &f.reviewer_token, target.id).await;
    let db = f._root.path().join("data/forge.sqlite");
    let held = f._root.path().join("data/held.sqlite");
    let original = std::fs::read(&db).unwrap();
    std::fs::rename(&db, &held).unwrap();
    refused(
        &f,
        "/reviews/dismiss",
        Some(&f.reviewer_token),
        request.clone(),
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    assert!(!db.exists());
    assert_eq!(std::fs::read(&held).unwrap(), original);
    std::fs::rename(&held, &db).unwrap();
    assert_eq!(
        f.request(
            HttpMethod::POST,
            "/reviews/dismiss",
            Some(&f.reviewer_token),
            request
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        f.core.list_reviews("alice", "reviewed", 1).unwrap().len(),
        2
    );
}

#[tokio::test]
async fn logout_cannot_report_success_or_expire_cookie_when_durable_revocation_fails() {
    let f = Fixture::new();
    let session_token = f.reviewer_cookie.strip_prefix("jeryu-session=").unwrap();
    let db = f._root.path().join("data/forge.sqlite");
    let held = f._root.path().join("data/held.sqlite");
    let before = std::fs::read(&db).unwrap();
    let request = || {
        Request::builder()
            .method(HttpMethod::POST)
            .uri("/api/v1/auth/logout")
            .header(header::COOKIE, &f.reviewer_cookie)
            .header("x-jeryu-csrf", &f.reviewer_csrf)
            .body(Body::empty())
            .unwrap()
    };
    std::fs::rename(&db, &held).unwrap();
    let response = f.router.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(response_json(response).await["code"], "writer_unavailable");
    assert!(f.core.authenticate_session(session_token).is_some());
    assert!(!db.exists());
    assert_eq!(std::fs::read(&held).unwrap(), before);
    std::fs::rename(&held, &db).unwrap();
    let response = f.router.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(response_json(response).await["ok"], true);
    assert!(f.core.authenticate_session(session_token).is_none());
}
