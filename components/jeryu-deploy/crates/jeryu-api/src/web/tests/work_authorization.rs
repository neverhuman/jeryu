//! Work authorization through the real authentication middleware and durable stores.

use super::*;
use axum::body::Body;
use axum::http::Request;
use jeryu_core::{CreateIssueRequest, Repository};
use jeryu_jira::{CreateWorkItemRequest, WorkItem, WorkRepository, WorkStore};
use std::process::Command;
use tower::ServiceExt;

struct Fixture {
    root: PathBuf,
    helper: String,
    identity: String,
    state: Option<WebState>,
    app: Option<AxumRouter>,
    tokens: BTreeMap<String, String>,
    permitted: Repository,
    private: Repository,
    bound: WorkItem,
    hidden: WorkItem,
    unbound: WorkItem,
}

fn custody_command(root: &Path, script: &str, arguments: &[&str]) -> String {
    let output = Command::new("/bin/bash")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .current_dir(root)
        .args([
            "-euo",
            "pipefail",
            "-c",
            script,
            "work-authorization-fixture",
        ])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "custody operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn reference(repo: &Repository) -> WorkRepository {
    WorkRepository {
        id: repo.id.to_string(),
        host: "jeryu".into(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
    }
}

fn state_at(root: &Path) -> WebState {
    let core = ForgeCore::open_sqlite(root.join("forge.sqlite")).unwrap();
    let mut state =
        WebState::new_with_git_storage(core, root.join("git")).with_auth(true, false, false);
    state.work = WorkStore::open(root.join("work.sqlite")).unwrap();
    state.github = crate::GithubRouter::with_core(state.core.clone())
        .with_repo_manager(state.repo_manager.clone())
        .with_work_store(state.work.clone());
    state
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("jeryu-work-authorization-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!(
            "Work authorization fixture retained until guarded success: {}",
            root.display()
        );
        let package = Path::new(env!("CARGO_MANIFEST_DIR"));
        let helper = [2, 4]
            .into_iter()
            .find_map(|depth| {
                let path = package.ancestors().nth(depth)?.join("tests/scratch.sh");
                path.is_file().then_some(path)
            })
            .expect("owning scratch guard");
        let helper = std::fs::read_to_string(helper).unwrap();
        let record = format!(
            "{helper}\njeryu_record_test_scratch \"$1\"\nprintf '%s' \"$jeryu_test_scratch_identity\"\n"
        );
        let identity = custody_command(&root, &record, &[root.to_str().unwrap()]);
        let state = state_at(&root);
        let mut tokens = BTreeMap::new();
        for login in ["admin", "writer", "reader", "outsider"] {
            state
                .core
                .create_account(
                    login,
                    "work-route-fixture-password",
                    if login == "admin" {
                        UserRole::Admin
                    } else {
                        UserRole::User
                    },
                )
                .unwrap();
            tokens.insert(
                login.to_string(),
                state
                    .core
                    .create_personal_access_token(
                        &super::credential_actor(&state.core, login, "work-route-fixture-password"),
                        "Work route fixture",
                        None,
                    )
                    .unwrap()
                    .secret,
            );
        }
        let create_repo = |name: &str| {
            state
                .core
                .create_repository(
                    "alice",
                    CreateRepositoryRequest {
                        name: name.into(),
                        private: true,
                        default_branch: Some("main".into()),
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        let permitted = create_repo("permitted");
        let private = create_repo("private");
        state
            .core
            .grant_repo_access(
                "admin",
                "writer",
                "alice",
                "permitted",
                RepoAccessLevel::Write,
            )
            .unwrap();
        state
            .core
            .grant_repo_access(
                "admin",
                "reader",
                "alice",
                "permitted",
                RepoAccessLevel::Read,
            )
            .unwrap();
        let create_item = |title: &str, repo: Option<WorkRepository>| {
            state
                .work
                .create(CreateWorkItemRequest {
                    title: title.into(),
                    repo,
                    ..Default::default()
                })
                .unwrap()
        };
        let bound = create_item("granted repository work", Some(reference(&permitted)));
        let hidden = create_item("private repository work", Some(reference(&private)));
        let unbound = create_item("administrator namespace work", None);
        let app = app(state.clone(), &root.join("absent-spa"));
        Self {
            root,
            helper,
            identity,
            state: Some(state),
            app: Some(app),
            tokens,
            permitted,
            private,
            bound,
            hidden,
            unbound,
        }
    }

    fn state(&self) -> &WebState {
        self.state.as_ref().unwrap()
    }

    fn snapshot(&self) -> Value {
        let work = self.state().work.list(Default::default()).unwrap();
        let details: Vec<_> = work
            .iter()
            .map(|item| self.state().work.detail(&item.key).unwrap())
            .collect();
        json!({"work":details,
            "permitted_issues":self.state().core.list_issues("alice", "permitted", None).unwrap(),
            "private_issues":self.state().core.list_issues("alice", "private", None).unwrap()})
    }

    async fn response(
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
            .as_ref()
            .unwrap()
            .clone()
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap()
    }

    async fn request(
        &self,
        method: HttpMethod,
        path: &str,
        actor: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = self.response(method, path, actor, body).await;
        let status = response.status();
        (status, response_json(response).await)
    }

    async fn refused(
        &self,
        method: HttpMethod,
        path: &str,
        actor: Option<&str>,
        body: Value,
        expected: StatusCode,
    ) {
        let before = self.snapshot();
        let (status, value) = self.request(method, path, actor, body).await;
        assert_eq!(status, expected, "unexpected response: {value}");
        assert_eq!(
            self.snapshot(),
            before,
            "denied Work request mutated durable state"
        );
    }

    fn restart(&mut self) {
        drop(self.app.take());
        drop(self.state.take());
        let state = state_at(&self.root);
        self.app = Some(app(state.clone(), &self.root.join("absent-spa")));
        self.state = Some(state);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Retain the WebState's existing auxiliary fixture guards too.
            if let Some(app) = self.app.take() {
                std::mem::forget(app);
            }
            if let Some(state) = self.state.take() {
                std::mem::forget(state);
            }
            eprintln!(
                "retaining failed Work authorization fixture: {}",
                self.root.display()
            );
            return;
        }
        drop(self.app.take());
        drop(self.state.take());
        let cleanup = format!(
            "{}\njeryu_test_scratch=\"$1\"\njeryu_test_scratch_identity=\"$2\"\njeryu_remove_test_scratch\n",
            self.helper
        );
        custody_command(
            Path::new("/"),
            &cleanup,
            &[self.root.to_str().unwrap(), &self.identity],
        );
    }
}

#[tokio::test]
async fn work_reads_use_real_auth_and_filter_private_and_unbound_items() {
    let f = Fixture::new();
    f.refused(
        HttpMethod::GET,
        "/api/v1/work",
        None,
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    for (actor, total) in [("admin", 3), ("writer", 1), ("reader", 1), ("outsider", 0)] {
        let (status, value) = f
            .request(HttpMethod::GET, "/api/v1/work", Some(actor), Value::Null)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["total"], total);
        if total == 1 {
            assert_eq!(value["items"][0]["key"], f.bound.key);
        }
    }
    for actor in ["writer", "reader", "outsider"] {
        for item in [&f.hidden, &f.unbound] {
            f.refused(
                HttpMethod::GET,
                &format!("/api/v1/work/{}", item.key),
                Some(actor),
                Value::Null,
                StatusCode::FORBIDDEN,
            )
            .await;
        }
        let (status, value) = f
            .request(
                HttpMethod::GET,
                &format!("/api/v1/work?repo_id={}", f.private.id),
                Some(actor),
                Value::Null,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["total"], 0);
    }
    let scoped = format!("/api/v1/repos/{}/work", f.permitted.id);
    f.refused(
        HttpMethod::GET,
        &scoped,
        Some("outsider"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        f.request(HttpMethod::GET, &scoped, Some("reader"), Value::Null)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        f.request(
            HttpMethod::GET,
            &format!("/api/v1/work/{}", f.unbound.key),
            Some("admin"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn work_mutations_require_write_grants_and_unbound_administration_before_side_effects() {
    let f = Fixture::new();
    for actor in [Some("reader"), Some("outsider"), None] {
        let expected = if actor.is_some() {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::UNAUTHORIZED
        };
        for (method, suffix, body) in [
            (
                HttpMethod::PATCH,
                "",
                json!({"title":"unauthorized change"}),
            ),
            (
                HttpMethod::POST,
                "/comments",
                json!({"body":"unauthorized comment","author":{"kind":"human","id":"admin"}}),
            ),
            (
                HttpMethod::POST,
                "/links",
                json!({"issue":{"owner":"alice","repo":"permitted","number":1}}),
            ),
        ] {
            f.refused(
                method,
                &format!("/api/v1/work/{}{suffix}", f.bound.key),
                actor,
                body,
                expected,
            )
            .await;
        }
        for path in [
            "/api/v1/work".to_string(),
            format!("/api/v1/repos/{}/work", f.permitted.id),
        ] {
            f.refused(
                HttpMethod::POST,
                &path,
                actor,
                json!({"title":"unauthorized create","repo":reference(&f.permitted)}),
                expected,
            )
            .await;
        }
    }
    for path in [
        "/api/v1/work".to_string(),
        format!("/api/v1/repos/{}/work", f.private.id),
    ] {
        f.refused(
            HttpMethod::POST,
            &path,
            Some("writer"),
            json!({"title":"private create","repo":reference(&f.private)}),
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    f.refused(
        HttpMethod::POST,
        "/api/v1/work",
        Some("writer"),
        json!({"title":"unbound create"}),
        StatusCode::FORBIDDEN,
    )
    .await;
    for item in [&f.hidden, &f.unbound] {
        for (method, suffix, body) in [
            (
                HttpMethod::PATCH,
                "",
                json!({"title":"unauthorized change"}),
            ),
            (
                HttpMethod::POST,
                "/comments",
                json!({"body":"unauthorized comment"}),
            ),
            (
                HttpMethod::POST,
                "/links",
                json!({"issue":{"owner":"alice","repo":"permitted","number":1}}),
            ),
        ] {
            f.refused(
                method,
                &format!("/api/v1/work/{}{suffix}", item.key),
                Some("writer"),
                body,
                StatusCode::FORBIDDEN,
            )
            .await;
        }
    }
    let mut orphaned = reference(&f.permitted);
    orphaned.id = "00000000-0000-0000-0000-000000000001".into();
    let mut mismatched = reference(&f.permitted);
    mismatched.name = "private".into();
    let mut alias = reference(&f.permitted);
    alias.id = "alice/permitted".into();
    let mut foreign = reference(&f.permitted);
    foreign.host = "foreign-forge".into();
    for (bad_ref, status) in [
        (orphaned, StatusCode::NOT_FOUND),
        (mismatched, StatusCode::UNPROCESSABLE_ENTITY),
        (alias, StatusCode::UNPROCESSABLE_ENTITY),
        (foreign, StatusCode::UNPROCESSABLE_ENTITY),
    ] {
        f.refused(
            HttpMethod::POST,
            "/api/v1/work",
            Some("writer"),
            json!({"title":"invalid repository identity","repo":bad_ref}),
            status,
        )
        .await;
        let malformed = f
            .state()
            .work
            .create(CreateWorkItemRequest {
                title: "orphaned binding".into(),
                repo: Some(bad_ref),
                ..Default::default()
            })
            .unwrap();
        f.refused(
            HttpMethod::GET,
            &format!("/api/v1/work/{}", malformed.key),
            Some("writer"),
            Value::Null,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    f.state()
        .core
        .delete_repository("alice", "permitted")
        .unwrap();
    let recreated = f
        .state()
        .core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "permitted".into(),
                private: false,
                default_branch: Some("main".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(recreated.id, f.permitted.id);
    f.state()
        .core
        .grant_repo_access(
            "admin",
            "writer",
            "alice",
            "permitted",
            RepoAccessLevel::Write,
        )
        .unwrap();
    assert!(
        f.state()
            .core
            .user_can_read_repo("writer", "alice", "permitted")
    );
    f.refused(
        HttpMethod::GET,
        &format!("/api/v1/work/{}", f.bound.key),
        Some("writer"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    f.refused(
        HttpMethod::PATCH,
        &format!("/api/v1/work/{}", f.bound.key),
        Some("writer"),
        json!({"title":"old private work"}),
        StatusCode::FORBIDDEN,
    )
    .await;
    f.refused(
        HttpMethod::POST,
        "/api/v1/work",
        Some("writer"),
        json!({"title":"stale create binding","repo":reference(&f.permitted)}),
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(
        f.request(
            HttpMethod::GET,
            &format!("/api/v1/repos/{}/work", recreated.id),
            Some("writer"),
            Value::Null
        )
        .await
        .1["total"],
        0
    );
}

#[tokio::test]
async fn work_writes_bind_authenticated_actors_and_survive_restart_and_grant_revocation() {
    let mut f = Fixture::new();
    f.refused(
        HttpMethod::POST,
        "/api/v1/work",
        Some("writer"),
        json!({"title":"invalid assignee","repo":reference(&f.permitted),"assignees":[{"kind":"human","id":" "}]}),
        StatusCode::UNPROCESSABLE_ENTITY,
    ).await;
    for path in [
        "/api/v1/work".to_string(),
        format!("/api/v1/repos/{}/work", f.permitted.id),
    ] {
        let (status, created) = f
            .request(
                HttpMethod::POST,
                &path,
                Some("writer"),
                json!({"title":"writer-created work","repo":reference(&f.permitted)}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let issue = f
            .state()
            .core
            .get_issue(
                "alice",
                "permitted",
                created["issue"]["number"].as_u64().unwrap(),
            )
            .unwrap();
        assert_eq!(issue.author, "writer");
    }
    let (status, created) = f
        .request(
            HttpMethod::POST,
            &format!("/api/v1/repos/{}/work", f.permitted.id),
            Some("writer"),
            json!({"title":"path owns the target","repo":reference(&f.private)}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["repo"]["id"], f.permitted.id.to_string());
    assert!(
        f.state()
            .core
            .list_issues("alice", "private", None)
            .unwrap()
            .is_empty()
    );
    let path = format!("/api/v1/work/{}", f.bound.key);
    assert_eq!(
        f.request(
            HttpMethod::PATCH,
            &path,
            Some("writer"),
            json!({"title":"durable authorized change"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, comment) = f.request(HttpMethod::POST, &format!("{path}/comments"), Some("writer"), json!({"body":"authenticated comment","author":{"kind":"agent","id":"admin","display_name":"Spoofed administrator"}})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(comment["author"]["kind"], "human");
    assert_eq!(comment["author"]["id"], "writer");
    assert_eq!(
        comment["author"]["display_name"],
        f.state().core.get_account("writer").unwrap().display_name
    );
    let (status, _) = f
        .request(
            HttpMethod::POST,
            "/api/v1/work",
            Some("admin"),
            json!({"title":"administrator unbound creation"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    f.restart();
    let (status, detail) = f
        .request(HttpMethod::GET, &path, Some("reader"), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["item"]["title"], "durable authorized change");
    assert_eq!(detail["comments"][0]["author"]["id"], "writer");
    f.state()
        .core
        .revoke_repo_access("reader", "alice", "permitted")
        .unwrap();
    f.refused(
        HttpMethod::GET,
        &path,
        Some("reader"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    f.restart();
    f.refused(
        HttpMethod::GET,
        &path,
        Some("reader"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn work_links_cannot_disclose_a_private_repository_to_unauthorized_readers() {
    let f = Fixture::new();
    let own_issue = f
        .state()
        .core
        .create_issue(
            "alice",
            "permitted",
            "writer",
            CreateIssueRequest {
                title: "same repository link".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let own_pull = f
        .state()
        .core
        .create_pull_request(
            "alice",
            "permitted",
            "writer",
            CreatePullRequestRequest {
                title: "same repository pull".into(),
                head: "feature".into(),
                base: "main".into(),
                head_sha: Some("a".repeat(40)),
                ..Default::default()
            },
        )
        .unwrap();
    let path = format!("/api/v1/work/{}/links", f.bound.key);
    let (status, linked) = f.request(HttpMethod::POST, &path, Some("writer"), json!({
        "issue":{"owner":"alice","repo":"permitted","number":own_issue.number,"url":"javascript:injected()"},
        "pull_request":{"owner":"alice","repo":"permitted","number":own_pull.number,"url":"javascript:injected()"}
    })).await;
    assert_eq!(status, StatusCode::OK, "{linked}");
    assert_eq!(
        linked["issue"]["url"],
        format!("/repos/jeryu/alice/permitted/issues#{}", own_issue.number)
    );
    assert_eq!(
        f.request(
            HttpMethod::GET,
            &format!("/api/v1/work/{}", f.bound.key),
            Some("reader"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    let issue = f
        .state()
        .core
        .create_issue(
            "alice",
            "private",
            "admin",
            CreateIssueRequest {
                title: "private linked issue".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let link = json!({"issue":{"owner":"alice","repo":"private","number":issue.number,"url":"javascript:injected()"}});
    f.refused(
        HttpMethod::POST,
        &path,
        Some("writer"),
        json!({"pull_request":{"owner":"alice","repo":"private","number":1}}),
        StatusCode::FORBIDDEN,
    )
    .await;
    f.refused(
        HttpMethod::POST,
        &path,
        Some("writer"),
        link.clone(),
        StatusCode::FORBIDDEN,
    )
    .await;
    let (status, linked) = f
        .request(HttpMethod::POST, &path, Some("admin"), link)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        linked["issue"]["url"],
        format!("/repos/jeryu/alice/private/issues#{}", issue.number)
    );
    f.refused(
        HttpMethod::GET,
        &format!("/api/v1/work/{}", f.bound.key),
        Some("reader"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        f.request(HttpMethod::GET, "/api/v1/work", Some("reader"), Value::Null)
            .await
            .1["total"],
        0
    );
    f.state()
        .core
        .grant_repo_access("admin", "reader", "alice", "private", RepoAccessLevel::Read)
        .unwrap();
    assert_eq!(
        f.request(
            HttpMethod::GET,
            &format!("/api/v1/work/{}", f.bound.key),
            Some("reader"),
            Value::Null
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

async fn assert_bridge_refuses_work_mutation(
    f: &Fixture,
    method: HttpMethod,
    path: &str,
    actor: &str,
    body: Value,
    status: StatusCode,
    key: &str,
) -> Value {
    let before = serde_json::to_value(f.state().work.detail(key).unwrap()).unwrap();
    let response = f.response(method, path, Some(actor), body).await;
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["x-jeryu-work-bridge"], "degraded");
    assert_eq!(
        response.headers()["x-jeryu-work-repair-code"],
        "work_bridge_repository_mismatch"
    );
    let result = response_json(response).await;
    assert_eq!(
        serde_json::to_value(f.state().work.detail(key).unwrap()).unwrap(),
        before
    );
    result
}

#[tokio::test]
async fn issue_bridge_cannot_mutate_work_after_repository_recreation() {
    let f = Fixture::new();
    let (status, old_issue) = f
        .request(
            HttpMethod::POST,
            "/repos/alice/permitted/issues",
            Some("writer"),
            json!({"title":"old private issue","actor":"admin"}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{old_issue}");
    assert_eq!(old_issue["number"], 1);
    let old_work = f
        .state()
        .work
        .find_by_issue("alice", "permitted", 1)
        .unwrap()
        .unwrap();
    f.state()
        .core
        .delete_repository("alice", "permitted")
        .unwrap();
    let replacement = f
        .state()
        .core
        .create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "permitted".into(),
                private: false,
                default_branch: Some("main".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(replacement.id, f.permitted.id);
    f.state()
        .core
        .grant_repo_access(
            "admin",
            "writer",
            "alice",
            "permitted",
            RepoAccessLevel::Write,
        )
        .unwrap();
    let issue = assert_bridge_refuses_work_mutation(
        &f,
        HttpMethod::POST,
        "/repos/alice/permitted/issues",
        "writer",
        json!({"title":"replacement issue"}),
        StatusCode::CREATED,
        &old_work.key,
    )
    .await;
    assert_eq!(issue["number"], 1);
    assert_bridge_refuses_work_mutation(
        &f,
        HttpMethod::PATCH,
        "/repos/alice/permitted/issues/1",
        "writer",
        json!({"title":"replacement update"}),
        StatusCode::OK,
        &old_work.key,
    )
    .await;
    assert_bridge_refuses_work_mutation(
        &f,
        HttpMethod::POST,
        "/repos/alice/permitted/issues/1/comments",
        "writer",
        json!({"body":"replacement comment","actor":"admin"}),
        StatusCode::CREATED,
        &old_work.key,
    )
    .await;
    assert_eq!(
        f.state()
            .core
            .get_issue("alice", "permitted", 1)
            .unwrap()
            .title,
        "replacement update"
    );
    f.refused(
        HttpMethod::GET,
        &format!("/api/v1/work/{}", old_work.key),
        Some("writer"),
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn issue_bridge_cannot_cross_work_repository_or_administrator_namespace() {
    let f = Fixture::new();
    f.state()
        .core
        .grant_repo_access(
            "admin",
            "outsider",
            "alice",
            "private",
            RepoAccessLevel::Write,
        )
        .unwrap();
    assert!(
        !f.state()
            .core
            .user_can_write_repo("outsider", "alice", "permitted")
    );
    let foreign_pull = f
        .state()
        .core
        .create_pull_request(
            "alice",
            "permitted",
            "writer",
            CreatePullRequestRequest {
                title: "private target unavailable to issue writer".into(),
                head: "feature".into(),
                base: "main".into(),
                head_sha: Some("b".repeat(40)),
                ..Default::default()
            },
        )
        .unwrap();
    // Admin-created links can point outside the representable ordinary-user
    // scope: a foreign owner, an unbound namespace, or an additional foreign PR.
    for (index, item) in [&f.bound, &f.unbound, &f.hidden].into_iter().enumerate() {
        let issue = f
            .state()
            .core
            .create_issue(
                "alice",
                "private",
                "admin",
                CreateIssueRequest {
                    title: format!("linked issue {index}"),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut link = json!({"issue":{"owner":"alice","repo":"private","number":issue.number}});
        if index == 2 {
            link["pull_request"] =
                json!({"owner":"alice","repo":"permitted","number":foreign_pull.number});
        }
        let (status, linked) = f
            .request(
                HttpMethod::POST,
                &format!("/api/v1/work/{}/links", item.key),
                Some("admin"),
                link,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{linked}");
        let path = format!("/repos/alice/private/issues/{}", issue.number);
        assert_bridge_refuses_work_mutation(
            &f,
            HttpMethod::PATCH,
            &path,
            "outsider",
            json!({"title":"authorized issue-only update"}),
            StatusCode::OK,
            &item.key,
        )
        .await;
        assert_bridge_refuses_work_mutation(
            &f,
            HttpMethod::POST,
            &format!("{path}/comments"),
            "outsider",
            json!({"body":"authorized issue-only comment"}),
            StatusCode::CREATED,
            &item.key,
        )
        .await;
        f.refused(
            HttpMethod::GET,
            &format!("/api/v1/work/{}", item.key),
            Some("outsider"),
            Value::Null,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
}
