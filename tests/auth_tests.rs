//! Critical audit fix #2 — production auth tests.
//!
//! Exercises the live `/api/v1/auth/login` + `/api/v1/auth/logout`
//! handlers end-to-end against a real `jeryu web serve` process bound
//! to an ephemeral loopback port. The BFF's `web` module is binary-
//! private (lives in `src/main.rs`'s module tree, not `lib.rs`), so the
//! only way to reach the live router from an integration test is
//! through a real HTTP listener — that's what each test below sets up
//! with [`ServeHandle::spawn`].

#![cfg(feature = "web")]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use reqwest::redirect;
use reqwest::{Client, StatusCode};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(45);

fn jeryu_bin() -> PathBuf {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        // The test binary may be invoked before `cargo build --features web`
        // has run jeryu's main bin. Build once per test process.
        let status = Command::new("cargo")
            .args([
                "build",
                "-p",
                "jeryu",
                "--bin",
                "jeryu",
                "--features",
                "web",
            ])
            .status()
            .expect("cargo build --bin jeryu --features web");
        assert!(status.success(), "failed to build jeryu binary");
    });
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_jeryu") {
        return PathBuf::from(p);
    }
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");
    PathBuf::from(manifest)
        .join("target")
        .join("debug")
        .join("jeryu")
}

/// Reserve a free localhost port. The listener is dropped before
/// returning so the spawned `jeryu web serve` can take the port; on
/// Linux this race window is harmless for serial test runs.
fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind 0");
    l.local_addr().expect("local_addr").port()
}

struct ServeHandle {
    child: Child,
    port: u16,
}

impl ServeHandle {
    async fn spawn(local_users: &str) -> Self {
        let bin = jeryu_bin();
        let port = free_port();
        let bind = format!("127.0.0.1:{port}");
        let tmp_spa = tempfile::TempDir::new().expect("tempdir");

        let mut cmd = Command::new(&bin);
        cmd.args([
            "web",
            "serve",
            "--bind",
            &bind,
            "--spa-dir",
            tmp_spa.path().to_str().expect("spa path utf-8"),
        ])
        .env("JERYU_LOCAL_USERS", local_users)
        // Ensure the dev-trust short-circuit is OFF — these tests
        // specifically verify the production session path.
        .env_remove("JERYU_WEB_TRUST_LOCAL")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

        let child = cmd.spawn().expect("spawn jeryu web serve");
        // Keep the tempdir alive for the child process lifetime.
        Box::leak(Box::new(tmp_spa));
        let h = ServeHandle { child, port };

        // Wait until the server starts answering.
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let client = test_client();
        let url = format!("http://127.0.0.1:{port}/api/v1/bootstrap");
        loop {
            if let Ok(resp) = client.get(&url).send().await {
                // Any HTTP response means the server is up. 401 is fine —
                // the bootstrap requires auth.
                let _ = resp;
                return h;
            }
            if Instant::now() > deadline {
                panic!("jeryu web serve did not bind within {STARTUP_TIMEOUT:?} on port {port}");
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for ServeHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn test_client() -> Client {
    Client::builder()
        .redirect(redirect::Policy::none())
        .build()
        .expect("reqwest client")
}

fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for v in headers.get_all(reqwest::header::SET_COOKIE).iter() {
        let s = v.to_str().ok()?;
        if let Some(rest) = s.strip_prefix(&format!("{name}="))
            && let Some((value, _)) = rest.split_once(';')
        {
            return Some(value.to_string());
        }
    }
    None
}

fn count_set_cookies(headers: &HeaderMap, name: &str) -> usize {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter(|v| {
            v.to_str()
                .map(|s| s.starts_with(&format!("{name}=")))
                .unwrap_or(false)
        })
        .count()
}

#[tokio::test(flavor = "current_thread")]
async fn local_login_returns_session_cookie_and_csrf_cookie() {
    let serve = ServeHandle::spawn("alice:repo.read,repo.write").await;
    let client = test_client();
    let resp = client
        .post(serve.url("/api/v1/auth/login"))
        .header("Content-Type", "application/json")
        .body(r#"{"provider":"local","login":"alice"}"#)
        .send()
        .await
        .expect("POST /api/v1/auth/login");

    assert_eq!(resp.status(), StatusCode::OK, "login should succeed");

    let headers = resp.headers().clone();
    assert_eq!(
        count_set_cookies(&headers, "__Host-jeryu-session"),
        1,
        "exactly one session Set-Cookie header"
    );
    assert_eq!(
        count_set_cookies(&headers, "__Host-jeryu-csrf"),
        1,
        "exactly one csrf Set-Cookie header"
    );

    let body: serde_json::Value = resp.json().await.expect("parse login response json");
    let perms = body["viewer"]["global_permissions"]
        .as_array()
        .expect("global_permissions array");
    let perms: Vec<&str> = perms.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(perms, vec!["repo.read", "repo.write"]);
    assert_eq!(body["viewer"]["login"], "alice");
}

#[tokio::test(flavor = "current_thread")]
async fn login_with_unknown_user_returns_403() {
    let serve = ServeHandle::spawn("alice:repo.read,repo.write").await;
    let client = test_client();
    let resp = client
        .post(serve.url("/api/v1/auth/login"))
        .header("Content-Type", "application/json")
        .body(r#"{"provider":"local","login":"bob"}"#)
        .send()
        .await
        .expect("POST login");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_accepts_valid_session() {
    let serve = ServeHandle::spawn("alice:repo.read,repo.write").await;
    let client = test_client();

    let resp = client
        .post(serve.url("/api/v1/auth/login"))
        .header("Content-Type", "application/json")
        .body(r#"{"provider":"local","login":"alice"}"#)
        .send()
        .await
        .expect("POST login");
    assert_eq!(resp.status(), StatusCode::OK);
    let session = extract_cookie_value(resp.headers(), "__Host-jeryu-session")
        .expect("session cookie present");

    let resp = client
        .get(serve.url("/api/v1/bootstrap"))
        .header(
            reqwest::header::COOKIE,
            format!("__Host-jeryu-session={session}"),
        )
        .send()
        .await
        .expect("GET bootstrap");
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test(flavor = "current_thread")]
async fn auth_layer_rejects_unknown_session() {
    let serve = ServeHandle::spawn("alice:repo.read,repo.write").await;
    let client = test_client();
    let resp = client
        .get(serve.url("/api/v1/bootstrap"))
        .header(
            reqwest::header::COOKIE,
            "__Host-jeryu-session=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        )
        .send()
        .await
        .expect("GET bootstrap");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn logout_revokes_session() {
    let serve = ServeHandle::spawn("alice:repo.read,repo.write").await;
    let client = test_client();

    let resp = client
        .post(serve.url("/api/v1/auth/login"))
        .header("Content-Type", "application/json")
        .body(r#"{"provider":"local","login":"alice"}"#)
        .send()
        .await
        .expect("POST login");
    assert_eq!(resp.status(), StatusCode::OK);
    let session = extract_cookie_value(resp.headers(), "__Host-jeryu-session")
        .expect("session cookie present");
    let csrf =
        extract_cookie_value(resp.headers(), "__Host-jeryu-csrf").expect("csrf cookie present");

    // Logout: needs the session cookie AND a matching X-CSRF-Token header.
    let resp = client
        .post(serve.url("/api/v1/auth/logout"))
        .header(
            reqwest::header::COOKIE,
            format!("__Host-jeryu-session={session}; __Host-jeryu-csrf={csrf}"),
        )
        .header("X-CSRF-Token", csrf.as_str())
        .send()
        .await
        .expect("POST logout");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // Old session must now fail.
    let resp = client
        .get(serve.url("/api/v1/bootstrap"))
        .header(
            reqwest::header::COOKIE,
            format!("__Host-jeryu-session={session}"),
        )
        .send()
        .await
        .expect("GET bootstrap after logout");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
