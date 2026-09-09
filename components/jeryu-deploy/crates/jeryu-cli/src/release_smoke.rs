//! Release smoke probe for a built `jeryu` binary.

use std::fs;
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, SET_COOKIE};
use serde::Serialize;
use serde_json::{Value, json};

const BOOTSTRAP_ADMIN_PASSWORD_ENV: &str = "JERYU_BOOTSTRAP_ADMIN_PASSWORD";

#[derive(Debug, Clone)]
struct AuthSession {
    cookie: String,
    csrf_token: String,
}

#[derive(Debug)]
struct ResponseCapture {
    status: u16,
    content_type: String,
    text: String,
    set_cookies: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProbeRecord {
    name: String,
    method: String,
    path: String,
    status: u16,
    expected_status: u16,
    content_type: String,
    ok: bool,
}

#[derive(Debug, Serialize)]
struct Receipt {
    schema_version: &'static str,
    status: &'static str,
    base_url: String,
    binary: String,
    spa_dir: String,
    probes: Vec<ProbeRecord>,
}

struct Server {
    child: Child,
}

impl Server {
    fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        self.child.try_wait().context("check server process")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn main() -> Result<()> {
    let binary = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/release/jeryu".to_string());
    ensure_executable(Path::new(&binary))?;
    let admin_password = std::env::var(BOOTSTRAP_ADMIN_PASSWORD_ENV)
        .with_context(|| format!("{BOOTSTRAP_ADMIN_PASSWORD_ENV} must be set for release smoke"))?;

    let root = std::env::current_dir().context("read current directory")?;
    let out_dir = env_path(
        "JERYU_RELEASE_ROUTE_PROBE_DIR",
        "target/artifact-support/route-probe",
    );
    let receipt_path = out_dir.join("receipt.json");
    let spa_dir = env_path(
        "JERYU_RELEASE_SMOKE_SPA_DIR",
        out_dir.join("missing-spa").to_string_lossy().as_ref(),
    );
    let log_path = out_dir.join("server.log");
    fs::create_dir_all(&out_dir).context("create route probe output directory")?;

    let port = reserve_loopback_port()?;
    let base_url = format!("http://127.0.0.1:{port}");
    let data_dir = tempfile::Builder::new()
        .prefix("jeryu-release-smoke.")
        .tempdir()
        .context("create smoke data directory")?;
    let mut server = spawn_server(
        Path::new(&binary),
        &root,
        port,
        data_dir.path(),
        &spa_dir,
        &log_path,
        &admin_password,
    )?;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("create HTTP client")?;
    let mut probes = Vec::new();
    wait_until_healthy(&client, &base_url, &mut server)?;

    probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "health",
            method: "GET",
            path: "/health",
            expected: 200,
            body: None,
            auth: None,
            accept: Some("application/json"),
            contains: Some("\"status\":\"ok\""),
            ctype_prefix: None,
        },
    );
    probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "embedded_spa_login",
            method: "GET",
            path: "/login",
            expected: 200,
            body: None,
            auth: None,
            accept: Some("text/html"),
            contains: Some("<div id=\"root\""),
            ctype_prefix: Some("text/html"),
        },
    );

    let admin_auth = auth_probe(
        &client,
        &base_url,
        &mut probes,
        "admin_login",
        "/api/v1/auth/login",
        json!({
            "login": "jeryu-admin",
            "password": admin_password
        }),
        None,
    )?;
    let admin_me = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "admin_me",
            method: "GET",
            path: "/api/v1/auth/me",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    require_auth_user(&admin_me, "jeryu-admin", "admin", false)?;
    let created_repo = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "admin_create_private_repo",
            method: "POST",
            path: "/repos",
            expected: 201,
            body: Some(json!({
                "name": "release-smoke-private",
                "private": true,
                "description": "Release smoke private repository",
                "default_branch": "main"
            })),
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    if created_repo.get("name").and_then(Value::as_str) != Some("release-smoke-private") {
        bail!("admin create repo returned unexpected body: {created_repo}");
    }
    let admin_repo_list = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "admin_repo_list",
            method: "GET",
            path: "/api/v1/repos",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    require_repo_list_len(&admin_repo_list, 1)?;
    let admin_users = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "admin_users",
            method: "GET",
            path: "/api/v1/admin/users",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    if !admin_users.as_array().is_some_and(|users| {
        users
            .iter()
            .any(|user| user.get("login").and_then(Value::as_str) == Some("jeryu-admin"))
    }) {
        bail!("admin user list did not include jeryu-admin: {admin_users}");
    }

    for spec in [
        Probe {
            name: "version",
            method: "GET",
            path: "/api/v1/version",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
        Probe {
            name: "bootstrap",
            method: "GET",
            path: "/api/v1/bootstrap",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
        Probe {
            name: "tool_registry",
            method: "GET",
            path: "/api/v1/tools/registry/summary",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
        Probe {
            name: "work_create",
            method: "POST",
            path: "/api/v1/work",
            expected: 201,
            body: Some(json!({
                "title": "Release smoke Work item",
                "kind": "task",
                "priority": "p2"
            })),
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
        Probe {
            name: "work_list",
            method: "GET",
            path: "/api/v1/work",
            expected: 200,
            body: None,
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
        Probe {
            name: "graphql_viewer",
            method: "POST",
            path: "/graphql",
            expected: 200,
            body: Some(json!({ "query": "query { viewer { login } }" })),
            auth: Some(&admin_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    ] {
        probe(&client, &base_url, &mut probes, spec);
    }

    let signup_auth = auth_probe(
        &client,
        &base_url,
        &mut probes,
        "signup",
        "/api/v1/auth/signup",
        json!({
            "login": "release-smoke",
            "password": "release-smoke-password-123"
        }),
        None,
    )?;
    let signup_me = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "signup_me",
            method: "GET",
            path: "/api/v1/auth/me",
            expected: 200,
            body: None,
            auth: Some(&signup_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    require_auth_user(&signup_me, "release-smoke", "user", false)?;
    let signup_repo_list = json_probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "signup_repo_list_empty",
            method: "GET",
            path: "/api/v1/repos",
            expected: 200,
            body: None,
            auth: Some(&signup_auth),
            accept: Some("application/json"),
            contains: None,
            ctype_prefix: None,
        },
    )?;
    require_repo_list_len(&signup_repo_list, 0)?;
    probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "signup_admin_users_denied",
            method: "GET",
            path: "/api/v1/admin/users",
            expected: 403,
            body: None,
            auth: Some(&signup_auth),
            accept: Some("application/json"),
            contains: Some("admin role required"),
            ctype_prefix: None,
        },
    );

    let relogin_auth = auth_probe(
        &client,
        &base_url,
        &mut probes,
        "login",
        "/api/v1/auth/login",
        json!({
            "login": "release-smoke",
            "password": "release-smoke-password-123"
        }),
        None,
    )?;
    probe(
        &client,
        &base_url,
        &mut probes,
        Probe {
            name: "logout",
            method: "POST",
            path: "/api/v1/auth/logout",
            expected: 200,
            body: Some(json!({})),
            auth: Some(&relogin_auth),
            accept: Some("application/json"),
            contains: Some("\"ok\":true"),
            ctype_prefix: None,
        },
    );

    let passed = probes.iter().all(|probe| probe.ok);
    write_receipt(
        &receipt_path,
        Receipt {
            schema_version: "jeryu.route-probe/v1",
            status: if passed { "passed" } else { "failed" },
            base_url,
            binary,
            spa_dir: spa_dir.to_string_lossy().to_string(),
            probes,
        },
    )?;

    if !passed {
        bail!("route probes failed; see {}", receipt_path.display());
    }
    println!(
        "[release-smoke] route probes passed -> {}",
        receipt_path.display()
    );
    Ok(())
}

struct Probe<'a> {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    expected: u16,
    body: Option<Value>,
    auth: Option<&'a AuthSession>,
    accept: Option<&'static str>,
    contains: Option<&'static str>,
    ctype_prefix: Option<&'static str>,
}

fn probe(client: &Client, base: &str, probes: &mut Vec<ProbeRecord>, spec: Probe<'_>) {
    let response = request(client, base, &spec);
    let mut ok = response.status == spec.expected;
    if let Some(expected) = spec.contains {
        ok = ok && response.text.contains(expected);
    }
    if let Some(prefix) = spec.ctype_prefix {
        ok = ok && response.content_type.starts_with(prefix);
    }
    probes.push(ProbeRecord {
        name: spec.name.to_string(),
        method: spec.method.to_string(),
        path: spec.path.to_string(),
        status: response.status,
        expected_status: spec.expected,
        content_type: response.content_type,
        ok,
    });
}

fn json_probe(
    client: &Client,
    base: &str,
    probes: &mut Vec<ProbeRecord>,
    spec: Probe<'_>,
) -> Result<Value> {
    let response = request(client, base, &spec);
    let mut ok = response.status == spec.expected;
    if let Some(expected) = spec.contains {
        ok = ok && response.text.contains(expected);
    }
    if let Some(prefix) = spec.ctype_prefix {
        ok = ok && response.content_type.starts_with(prefix);
    }
    probes.push(ProbeRecord {
        name: spec.name.to_string(),
        method: spec.method.to_string(),
        path: spec.path.to_string(),
        status: response.status,
        expected_status: spec.expected,
        content_type: response.content_type.clone(),
        ok,
    });
    if !ok {
        bail!(
            "{} failed with status {} body {}",
            spec.name,
            response.status,
            response.text
        );
    }
    serde_json::from_str(&response.text)
        .with_context(|| format!("parse JSON response for {}", spec.name))
}

fn require_auth_user(value: &Value, login: &str, role: &str, must_change: bool) -> Result<()> {
    if value.get("login").and_then(Value::as_str) != Some(login)
        || value.get("role").and_then(Value::as_str) != Some(role)
        || value.get("mustChangePassword").and_then(Value::as_bool) != Some(must_change)
    {
        bail!("unexpected auth user response: {value}");
    }
    Ok(())
}

fn require_repo_list_len(value: &Value, expected: usize) -> Result<()> {
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .context("repo list response missing total")?;
    let repositories = value
        .get("repositories")
        .and_then(Value::as_array)
        .context("repo list response missing repositories")?;
    if total != expected as u64 || repositories.len() != expected {
        bail!("expected {expected} repositories, got {value}");
    }
    Ok(())
}

fn auth_probe(
    client: &Client,
    base: &str,
    probes: &mut Vec<ProbeRecord>,
    name: &'static str,
    path: &'static str,
    body: Value,
    auth: Option<&AuthSession>,
) -> Result<AuthSession> {
    let spec = Probe {
        name,
        method: "POST",
        path,
        expected: 200,
        body: Some(body),
        auth,
        accept: Some("application/json"),
        contains: None,
        ctype_prefix: None,
    };
    let response = request(client, base, &spec);
    let ok = response.status == spec.expected;
    probes.push(ProbeRecord {
        name: name.to_string(),
        method: spec.method.to_string(),
        path: path.to_string(),
        status: response.status,
        expected_status: spec.expected,
        content_type: response.content_type.clone(),
        ok,
    });
    if !ok {
        bail!("{name} failed with status {}", response.status);
    }
    let cookie = response
        .set_cookies
        .iter()
        .filter_map(|value| value.split(';').next())
        .find(|value| {
            value.starts_with("__Host-jeryu-session=") || value.starts_with("jeryu-session=")
        })
        .map(str::to_string)
        .context("auth response did not set a session cookie")?;
    let body: Value = serde_json::from_str(&response.text).context("parse auth response")?;
    let csrf_token = body
        .get("csrfToken")
        .and_then(Value::as_str)
        .context("auth response did not include csrfToken")?
        .to_string();
    Ok(AuthSession { cookie, csrf_token })
}

fn request(client: &Client, base: &str, spec: &Probe<'_>) -> ResponseCapture {
    let url = format!("{}{}", base, spec.path);
    let method = spec
        .method
        .parse()
        .expect("static smoke HTTP method is valid");
    let mut request = client.request(method, url);
    if let Some(accept) = spec.accept {
        request = request.header(ACCEPT, accept);
    }
    if let Some(auth) = spec.auth {
        request = request.header(COOKIE, &auth.cookie);
        if !matches!(spec.method, "GET" | "HEAD" | "OPTIONS") {
            request = request.header("x-jeryu-csrf", &auth.csrf_token);
        }
    }
    if let Some(body) = &spec.body {
        request = request.header(CONTENT_TYPE, "application/json").json(body);
    }
    match request.send() {
        Ok(response) => capture_response(response),
        Err(error) => ResponseCapture {
            status: 0,
            content_type: String::new(),
            text: error.to_string(),
            set_cookies: Vec::new(),
        },
    }
}

fn capture_response(response: reqwest::blocking::Response) -> ResponseCapture {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let content_type = header_text(&headers, CONTENT_TYPE.as_str()).unwrap_or_default();
    let set_cookies = headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect();
    let text = response
        .text()
        .unwrap_or_else(|error| format!("response body read failed: {error}"));
    ResponseCapture {
        status,
        content_type,
        text,
        set_cookies,
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value: &HeaderValue| value.to_str().ok())
        .map(str::to_string)
}

fn wait_until_healthy(client: &Client, base: &str, server: &mut Server) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let health = Probe {
        name: "health_wait",
        method: "GET",
        path: "/health",
        expected: 200,
        body: None,
        auth: None,
        accept: Some("application/json"),
        contains: None,
        ctype_prefix: None,
    };
    loop {
        if request(client, base, &health).status == 200 {
            return Ok(());
        }
        if let Some(status) = server.try_wait()? {
            bail!("server exited before becoming healthy: {status}");
        }
        if Instant::now() >= deadline {
            bail!("server did not become healthy within 20 seconds");
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn spawn_server(
    binary: &Path,
    root: &Path,
    port: u16,
    data_dir: &Path,
    spa_dir: &Path,
    log_path: &Path,
    admin_password: &str,
) -> Result<Server> {
    let log = fs::File::create(log_path).context("create server log")?;
    let stderr = log.try_clone().context("clone server log handle")?;
    let child = Command::new(binary)
        .current_dir(root)
        .env_remove("JERYU_WEB_TRUST_LOCAL")
        .env(BOOTSTRAP_ADMIN_PASSWORD_ENV, admin_password)
        .args([
            "serve",
            "--bind",
            &format!("127.0.0.1:{port}"),
            "--data-dir",
        ])
        .arg(data_dir)
        .arg("--spa-dir")
        .arg(spa_dir)
        .args(["--split-manifest", "repos.manifest.toml"])
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("start release binary")?;
    Ok(Server { child })
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("reserve loopback port")?;
    let port = listener.local_addr().context("read reserved port")?.port();
    drop(listener);
    Ok(port)
}

fn env_path(name: &str, default: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

fn ensure_executable(path: &Path) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("binary does not exist: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("binary is not a file: {}", path.display());
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        bail!("binary is not executable: {}", path.display());
    }
    Ok(())
}

fn write_receipt(path: &Path, receipt: Receipt) -> Result<()> {
    let text = serde_json::to_string_pretty(&receipt).context("serialize receipt")?;
    fs::write(path, format!("{text}\n")).context("write route-probe receipt")
}
