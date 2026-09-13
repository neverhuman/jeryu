//! Smart-HTTP authorization against resolved storage identities and explicit credentials.

use super::*;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use jeryu_gitd::RepoId;
use std::net::SocketAddr;
use std::process::Command;
use tower::ServiceExt;

struct Fixture {
    root: PathBuf,
    helper: String,
    identity: String,
    state: Option<WebState>,
    app: Option<AxumRouter>,
    tokens: BTreeMap<String, String>,
    writer_cookie: String,
}

fn custody(root: &Path, script: &str, arguments: &[&str]) -> String {
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
            "git-authorization-fixture",
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

impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("jeryu-git-authorization-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap()
            .keep();
        eprintln!(
            "Git authorization fixture retained until guarded success: {}",
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
        let identity = custody(&root, &record, &[root.to_str().unwrap()]);
        let core = ForgeCore::open_sqlite(root.join("forge.sqlite")).unwrap();
        let state =
            WebState::new_with_git_storage(core, root.join("git")).with_auth(true, false, false);
        let mut fixture = Self {
            root,
            helper,
            identity,
            state: Some(state),
            app: None,
            tokens: BTreeMap::new(),
            writer_cookie: String::new(),
        };
        let state = fixture.state.as_ref().unwrap();
        for login in ["admin", "writer", "shadow"] {
            state
                .core
                .create_account(
                    login,
                    "git-route-fixture-password",
                    if login == "admin" {
                        UserRole::Admin
                    } else {
                        UserRole::User
                    },
                )
                .unwrap();
            fixture.tokens.insert(
                login.to_string(),
                state
                    .core
                    .create_personal_access_token(
                        &super::credential_actor(&state.core, login, "git-route-fixture-password"),
                        "Git authorization fixture",
                        None,
                    )
                    .unwrap()
                    .secret,
            );
        }
        // The REST compatibility surface can represent the shadow metadata row.
        // Only secret and public have storage: secret.git.git resolves to secret.
        for (name, private) in [("secret", true), ("secret.git", false), ("public", false)] {
            state
                .core
                .create_repository(
                    "alice",
                    CreateRepositoryRequest {
                        name: name.into(),
                        private,
                        default_branch: Some("main".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        for name in ["secret", "public"] {
            state
                .repo_manager
                .create_bare(&RepoId::new("alice", name).unwrap())
                .unwrap();
            state
                .core
                .grant_repo_access("admin", "writer", "alice", name, RepoAccessLevel::Write)
                .unwrap();
        }
        state
            .core
            .grant_repo_access(
                "admin",
                "shadow",
                "alice",
                "secret.git",
                RepoAccessLevel::Write,
            )
            .unwrap();
        fixture.writer_cookie = format!(
            "jeryu-session={}",
            state
                .core
                .create_session("writer", "git-route-fixture-password")
                .unwrap()
                .token
        );
        fixture.app = Some(app(state.clone(), &fixture.root.join("absent-spa")));
        fixture
    }

    fn bearer(&self, actor: &str) -> String {
        format!("Bearer {}", self.tokens[actor])
    }

    async fn response(
        &self,
        method: HttpMethod,
        path: &str,
        authorization: Option<&str>,
        cookie: Option<&str>,
    ) -> AxumResponse {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        if let Some(value) = cookie {
            request = request.header(header::COOKIE, value);
        }
        let mut request = request.body(Body::empty()).unwrap();
        // Exercise actual routing and the transport's mandatory peer extractor.
        // This is a documentation-range remote address; local bypass is disabled.
        request.extensions_mut().insert(ConnectInfo(
            "192.0.2.1:43210".parse::<SocketAddr>().unwrap(),
        ));
        self.app
            .as_ref()
            .unwrap()
            .clone()
            .oneshot(request)
            .await
            .unwrap()
    }

    fn refs(&self, name: &str) -> Vec<jeryu_gitd::refs::GitRef> {
        let manager = self.state.as_ref().unwrap().repo_manager.as_ref().clone();
        let repo = manager.open_parts("alice", name).unwrap();
        jeryu_gitd::refs::RefService::new(manager)
            .list_refs(&repo)
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(app) = self.app.take() {
                std::mem::forget(app);
            }
            if let Some(state) = self.state.take() {
                std::mem::forget(state);
            }
            eprintln!(
                "retaining failed Git authorization fixture: {}",
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
        custody(
            Path::new("/"),
            &cleanup,
            &[self.root.to_str().unwrap(), &self.identity],
        );
    }
}

#[tokio::test]
async fn git_public_and_grant_shadows_cannot_authorize_a_private_storage_target() {
    let f = Fixture::new();
    let state = f.state.as_ref().unwrap();
    assert!(
        !state
            .core
            .get_repository("alice", "secret.git")
            .unwrap()
            .private
    );
    assert!(
        state
            .core
            .get_repository("alice", "secret")
            .unwrap()
            .private
    );
    assert!(
        state
            .core
            .user_can_write_repo("shadow", "alice", "secret.git")
    );
    assert!(!state.core.user_can_read_repo("shadow", "alice", "secret"));
    assert_eq!(
        state
            .repo_manager
            .resolve_parts("alice", "secret.git.git")
            .unwrap()
            .id
            .name,
        "secret"
    );
    let before = f.refs("secret");
    let shadow = f.bearer("shadow");
    let writer = f.bearer("writer");
    for name in ["secret.git", "secret.git.git", "secret.git.git.git"] {
        for service in ["git-upload-pack", "git-receive-pack"] {
            let path = format!("/git/alice/{name}/info/refs?service={service}");
            assert_eq!(
                f.response(HttpMethod::GET, &path, None, None)
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED,
                "anonymous {path}"
            );
            assert_eq!(
                f.response(HttpMethod::GET, &path, Some(&shadow), None)
                    .await
                    .status(),
                StatusCode::FORBIDDEN,
                "shadow principal {path}"
            );
            // The same resolved target is usable with its actual writer's grant.
            let response = f
                .response(HttpMethod::GET, &path, Some(&writer), None)
                .await;
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "authorized writer {path}"
            );
            assert_eq!(
                response.headers()[header::CONTENT_TYPE].to_str().unwrap(),
                format!("application/x-{service}-advertisement")
            );
        }
        let path = format!("/git/alice/{name}/git-receive-pack");
        assert_eq!(
            f.response(HttpMethod::POST, &path, Some(&shadow), None)
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(f.refs("secret"), before, "denied shadow push changed refs");
    let public = "/git/alice/public.git/info/refs?service=git-upload-pack";
    assert_eq!(
        f.response(HttpMethod::GET, public, None, None)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.response(HttpMethod::GET, public, Some(&shadow), None)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn git_explicit_invalid_credentials_cannot_fall_back_to_a_valid_session() {
    let f = Fixture::new();
    let before = (f.refs("secret"), f.refs("public"));
    let invalid = "Bearer invalid-git-route-fixture-token";
    for name in ["secret", "public"] {
        for service in ["git-upload-pack", "git-receive-pack"] {
            let path = format!("/git/alice/{name}.git/info/refs?service={service}");
            // Positive control: the session really authorizes this target/service.
            assert_eq!(
                f.response(HttpMethod::GET, &path, None, Some(&f.writer_cookie))
                    .await
                    .status(),
                StatusCode::OK
            );
            let response = f
                .response(
                    HttpMethod::GET,
                    &path,
                    Some(invalid),
                    Some(&f.writer_cookie),
                )
                .await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(
                response.headers()[header::WWW_AUTHENTICATE],
                "Basic realm=\"jeryu\""
            );
            let rpc = format!("/git/alice/{name}.git/{service}");
            assert_eq!(
                f.response(
                    HttpMethod::POST,
                    &rpc,
                    Some(invalid),
                    Some(&f.writer_cookie)
                )
                .await
                .status(),
                StatusCode::UNAUTHORIZED,
                "{rpc}"
            );
        }
    }
    // A valid but unrelated PAT also must not borrow the cookie owner's grant.
    let shadow = f.bearer("shadow");
    let write = "/git/alice/secret.git/info/refs?service=git-receive-pack";
    assert_eq!(
        f.response(
            HttpMethod::GET,
            write,
            Some(&shadow),
            Some(&f.writer_cookie)
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        (f.refs("secret"), f.refs("public")),
        before,
        "denied credential requests changed refs"
    );
}
