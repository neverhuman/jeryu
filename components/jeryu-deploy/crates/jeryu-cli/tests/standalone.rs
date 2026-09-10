//! Process-level standalone proof using empty configuration and durable storage.

use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    net::TcpListener,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn private_temp_dir() -> TempDir {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn isolated_command(home: &Path) -> Command {
    let binary =
        std::env::var_os("JERYU_TEST_BINARY").unwrap_or_else(|| env!("CARGO_BIN_EXE_jeryu").into());
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", home)
        .current_dir(home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    command
}

fn server_command(home: &Path, data: &Path, address: &str, initialize: bool) -> Command {
    let mut command = isolated_command(home);
    command
        .args(["serve", "--bind", address])
        .env("JERYU_DATA_DIR", data)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if initialize {
        command.env(
            "JERYU_BOOTSTRAP_ADMIN_PASSWORD",
            "standalone-fixture-password",
        );
    }
    command
}

fn start(home: &Path, data: &Path, address: &str, initialize: bool) -> Server {
    wait_until_ready(server_command(home, data, address, initialize), address)
}

fn wait_until_ready(mut command: Command, address: &str) -> Server {
    let mut server = Server(command.spawn().unwrap());
    let client = Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    // Account creation hashes a password; allow contention in the full matrix.
    // A stopped child still fails immediately, and readiness is always probed.
    for _ in 0..600 {
        assert!(
            server.0.try_wait().unwrap().is_none(),
            "server stopped before readiness"
        );
        if client
            .get(format!("http://{address}/health"))
            .send()
            .is_ok_and(|r| r.status().is_success())
        {
            return server;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not become ready");
}

fn cli(home: &Path, url: &str, token: &str, args: &[&str]) -> Value {
    let output = isolated_command(home)
        .env("JERYU_API_URL", url)
        .env("JERYU_TOKEN", token)
        .args(["--owner", "jeryu-admin", "--json"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn git_output(directory: &Path, token: &str, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "http.extraHeader")
        .env(
            "GIT_CONFIG_VALUE_0",
            format!("Authorization: Bearer {token}"),
        )
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

fn git(directory: &Path, token: &str, args: &[&str]) -> String {
    let output = git_output(directory, token, args);
    assert!(
        output.status.success(),
        "Git operation {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn request(
    http: &Client,
    method: Method,
    url: &str,
    token: &str,
    body: Value,
    status: u16,
) -> Value {
    let response = http
        .request(method, url)
        .bearer_auth(token)
        .header("idempotency-key", "protected-process-fixture-repository")
        .json(&body)
        .send()
        .unwrap();
    let actual = response.status().as_u16();
    let body = response.text().unwrap();
    assert_eq!(actual, status, "request {url}: {body}");
    serde_json::from_str(&body).unwrap()
}

fn account_token(http: &Client, url: &str, login: &str, signup: bool) -> String {
    let route = if signup { "signup" } else { "login" };
    let response = http
        .post(format!("{url}/api/v1/auth/{route}"))
        .json(&json!({"login":login,"password":"standalone-fixture-password"}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let authenticated: Value = response.json().unwrap();
    let token: Value = http
        .post(format!("{url}/api/v1/auth/tokens"))
        .header("cookie", cookie)
        .header("x-jeryu-csrf", authenticated["csrfToken"].as_str().unwrap())
        .json(&json!({"name":"protected-pr-process-test"}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    token["token"].as_str().unwrap().to_owned()
}

#[test]
fn ci_status_reads_authorized_check_evidence_across_restart() {
    let temp = private_temp_dir();
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    std::fs::create_dir(&home).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let url = format!("http://{address}");
    let server = start(&home, &data, &address, true);
    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let admin = account_token(&http, &url, "jeryu-admin", false);
    let reader = account_token(&http, &url, "ci-reader", true);
    let unrelated = account_token(&http, &url, "ci-unrelated", true);
    request(
        &http,
        Method::POST,
        &format!("{url}/api/v1/repos"),
        &admin,
        json!({"host":"jeryu","owner":"jeryu-admin","name":"check-evidence",
            "visibility":"private","initialize_readme":true,"default_branch":"main",
            "topics":[],"dry_run":false}),
        201,
    );
    request(
        &http,
        Method::POST,
        &format!("{url}/api/v1/admin/repos/jeryu-admin/check-evidence/grants/ci-reader"),
        &admin,
        json!({"access":"read"}),
        200,
    );
    let args = ["ci", "status", "--repo", "check-evidence"];
    assert_eq!(
        cli(&home, &url, &reader, &args),
        json!({"total_count":0,"check_runs":[]})
    );
    let mut checks = Vec::new();
    for (index, (status, conclusion)) in [
        ("queued", Value::Null),
        ("in_progress", Value::Null),
        ("completed", json!("failure")),
        ("completed", json!("skipped")),
    ]
    .into_iter()
    .enumerate()
    {
        // Local fixture evidence only; UUIDs are allocated by the real server.
        checks.push(request(
            &http,
            Method::POST,
            &format!("{url}/repos/jeryu-admin/check-evidence/check-runs"),
            &admin,
            json!({"name":format!("fixture/check-{index}"),"head_sha":format!("{index:040x}"),
                "status":status,"conclusion":conclusion,
                "details_url":"https://example.invalid/fixture",
                "output":{"title":"fixture","summary":"retained evidence","text":"detail"}}),
            201,
        ));
    }
    let expected = json!({"total_count":checks.len(),"check_runs":checks});
    assert_eq!(cli(&home, &url, &admin, &args), expected);
    assert_eq!(cli(&home, &url, &reader, &args), expected);
    for (token, repo, status) in [
        (None, "check-evidence", 401),
        (Some(unrelated.as_str()), "check-evidence", 403),
        (Some(admin.as_str()), "missing-evidence", 404),
    ] {
        let mut command = isolated_command(&home);
        command.env("JERYU_API_URL", &url);
        if let Some(token) = token {
            command.env("JERYU_TOKEN", token);
        }
        let output = command
            .args([
                "--owner",
                "jeryu-admin",
                "--json",
                "ci",
                "status",
                "--repo",
                repo,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(&format!("HTTP {status}")));
    }
    drop(server);
    let _restarted = start(&home, &data, &address, false);
    assert_eq!(cli(&home, &url, &reader, &args), expected);
}

#[test]
fn authenticated_protected_review_checks_merge_and_restart_preserve_exact_head() {
    let temp = private_temp_dir();
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    let source = temp.path().join("git-client");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&source).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let url = format!("http://{address}");
    let server = start(&home, &data, &address, true);
    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let publisher = account_token(&http, &url, "jeryu-admin", false);
    let author = account_token(&http, &url, "fixture-author", true);
    let reviewer = account_token(&http, &url, "fixture-reviewer", true);
    let merger = account_token(&http, &url, "fixture-merger", true);
    let repo = request(
        &http,
        Method::POST,
        &format!("{url}/api/v1/repos"),
        &publisher,
        json!({"host":"jeryu","owner":"jeryu-admin","name":"protected-repo",
            "visibility":"private","initialize_readme":true,"default_branch":"main",
            "topics":[],"dry_run":false}),
        201,
    );
    let repo_url = format!("{url}/repos/jeryu-admin/protected-repo");
    for login in ["fixture-author", "fixture-reviewer", "fixture-merger"] {
        request(
            &http,
            Method::POST,
            &format!("{url}/api/v1/admin/repos/jeryu-admin/protected-repo/grants/{login}"),
            &publisher,
            json!({"access":"write"}),
            200,
        );
    }
    let git_url = format!("{url}/git/jeryu-admin/protected-repo.git");
    git(
        temp.path(),
        &author,
        &[
            "clone",
            "--branch",
            "main",
            &git_url,
            source.to_str().unwrap(),
        ],
    );
    git(&source, &author, &["config", "user.name", "Fixture Author"]);
    git(
        &source,
        &author,
        &["config", "user.email", "author@example.invalid"],
    );
    let base = git(&source, &author, &["rev-parse", "HEAD"]);
    git(&source, &author, &["checkout", "-b", "feature"]);
    std::fs::write(source.join("README.md"), "reviewed source\n").unwrap();
    git(&source, &author, &["commit", "-am", "Proposed feature"]);
    let head = git(&source, &author, &["rev-parse", "HEAD"]);
    git(&source, &author, &["push", &git_url, "feature"]);
    let pr = request(
        &http,
        Method::POST,
        &format!("{repo_url}/pulls"),
        &author,
        json!({"title":"Protected process proof","head":"feature","base":"main","actor":"spoofed-author"}),
        201,
    );
    assert_eq!(pr["user"]["login"], "fixture-author");
    assert_eq!(pr["head"]["sha"], head);
    assert_eq!(pr["base"]["sha"], base);
    let number = pr["number"].as_u64().unwrap();
    request(
        &http,
        Method::PUT,
        &format!("{repo_url}/branches/main/protection"),
        &publisher,
        json!({"required_approving_review_count":1,"required_status_checks":["fixture/required"],
            "enforce_admins":true,"required_linear_history":true}),
        200,
    );
    let direct = git_output(
        &source,
        &author,
        &["push", &git_url, "HEAD:refs/heads/main"],
    );
    assert!(
        !direct.status.success(),
        "direct protected-main push was accepted"
    );
    let merge_url = format!("{repo_url}/pulls/{number}/merge");
    let merge = json!({"sha":head,"merge_method":"merge"});
    request(&http, Method::PUT, &merge_url, &merger, merge.clone(), 405);

    // These are isolated fixture checks, never production CI publication.
    let checks_url = format!("{repo_url}/check-runs");
    let check = json!({"name":"fixture/required","head_sha":head,"status":"completed","conclusion":"success"});
    request(
        &http,
        Method::POST,
        &checks_url,
        &author,
        check.clone(),
        403,
    );
    request(
        &http,
        Method::POST,
        &checks_url,
        &publisher,
        json!({"name":"fixture/required","head_sha":base,"status":"completed","conclusion":"success"}),
        201,
    );
    let exact_checks = request(
        &http,
        Method::GET,
        &format!("{repo_url}/commits/{head}/check-runs"),
        &merger,
        Value::Null,
        200,
    );
    assert!(
        exact_checks["check_runs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|run| run["head_sha"] == head)
    );

    let detail_url = format!(
        "{url}/api/v1/repos/{}/pulls/{number}",
        repo["id"]["id"].as_str().unwrap()
    );
    let approve_url = format!("{detail_url}/approve");
    request(
        &http,
        Method::POST,
        &approve_url,
        &author,
        json!({"expected_head_sha":head}),
        403,
    );
    request(
        &http,
        Method::POST,
        &approve_url,
        &reviewer,
        json!({"expected_head_sha":base}),
        409,
    );
    request(
        &http,
        Method::POST,
        &approve_url,
        &reviewer,
        json!({"expected_head_sha":head,"actor":"spoofed-reviewer"}),
        200,
    );
    request(&http, Method::PUT, &merge_url, &merger, merge.clone(), 405);
    request(&http, Method::POST, &checks_url, &publisher, check, 201);
    drop(server);

    let server = start(&home, &data, &address, false);
    let detail = request(&http, Method::GET, &detail_url, &merger, Value::Null, 200);
    let reviews = detail["reviews"].as_array().unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0]["author"], "fixture-reviewer");
    assert_eq!(reviews[0]["head_sha"], head);
    assert_eq!(reviews[0]["effective"], true);
    let merged = request(&http, Method::PUT, &merge_url, &merger, merge, 200);
    assert_eq!(merged["sha"], head);
    drop(server);

    let _server = start(&home, &data, &address, false);
    let final_pr = request(
        &http,
        Method::GET,
        &format!("{repo_url}/pulls/{number}"),
        &merger,
        Value::Null,
        200,
    );
    assert_eq!(final_pr["merged"], true);
    assert_eq!(final_pr["merge_commit_sha"], head);
    let clone = temp.path().join("verified-main");
    git(
        temp.path(),
        &merger,
        &[
            "clone",
            "--branch",
            "main",
            &git_url,
            clone.to_str().unwrap(),
        ],
    );
    assert_eq!(git(&clone, &merger, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap(),
        "reviewed source\n"
    );
}

#[test]
fn startup_cli_and_restart_use_durable_state_from_any_directory() {
    let temp = private_temp_dir();
    let home = temp.path().join("home");
    let data = temp.path().join("durable");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("index.html"), "untrusted-current-directory").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let url = format!("http://{address}");
    let server = start(&home, &data, &address, true);
    let http = Client::new();
    let page = http.get(format!("{url}/login")).send().unwrap();
    assert!(
        page.status().is_success(),
        "embedded web bundle must be built before the runtime lane"
    );
    assert!(!page.text().unwrap().contains("untrusted-current-directory"));

    // These exact public assets must survive the source build and binary embedding.
    // Hashing also rejects the SPA fallback, which returns HTML for a missing path.
    for (path, content_type, sha256) in [
        (
            "fonts/OFL.txt",
            "text/plain; charset=utf-8",
            "b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d",
        ),
        (
            "DEPENDENCY_NOTICES.txt",
            "text/plain; charset=utf-8",
            "a192a8107d222d6c85f8f027edfa40fd4b1275c7be31852eb89be70d9dcd2acc",
        ),
        (
            "fonts/JetBrainsMono-400.woff2",
            "application/octet-stream",
            "14425ba9c695763c1547f48a206b7aa60350a33ae23de09f0407877f3fcd89eb",
        ),
        (
            "fonts/JetBrainsMono-500.woff2",
            "application/octet-stream",
            "cb182feeed4d798ff6961d3c79f7026279448fca0676438aaecb21f3fc39553a",
        ),
        (
            "fonts/JetBrainsMono-700.woff2",
            "application/octet-stream",
            "d0d4e818808f2a0ba39b2b09d1989366f63494e295f003c7ef436697378507e8",
        ),
    ] {
        let response = http.get(format!("{url}/{path}")).send().unwrap();
        assert_eq!(response.status(), 200, "embedded asset {path}");
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some(content_type),
            "embedded asset {path}"
        );
        let bytes = response.bytes().unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&bytes)),
            sha256,
            "embedded asset {path}"
        );
    }
    let notice = http
        .get(format!("{url}/THIRD_PARTY_NOTICES.txt"))
        .send()
        .unwrap();
    assert_eq!(notice.status(), 200);
    assert_eq!(
        notice
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    let notice = notice.text().unwrap();
    assert!(notice.contains("JetBrains Mono"));
    assert!(notice.contains("SIL Open Font License, Version 1.1"));
    assert!(notice.contains("fonts/OFL.txt"));

    let login = http
        .post(format!("{url}/api/v1/auth/login"))
        .json(&json!({"login":"jeryu-admin","password":"standalone-fixture-password"}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login: Value = login.json().unwrap();
    let token: Value = http
        .post(format!("{url}/api/v1/auth/tokens"))
        .header("cookie", cookie)
        .header("x-jeryu-csrf", login["csrfToken"].as_str().unwrap())
        .json(&json!({"name":"standalone-test"}))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let token = token["token"].as_str().unwrap();
    let runners = cli(&home, &url, token, &["runners", "status"]);
    assert_eq!(runners["local"]["state"], "unknown");
    assert_eq!(runners["local"]["nodes"], 0);
    assert_eq!(runners["local"]["totalSlots"], 0);
    assert_eq!(runners["local"]["activeSlots"], 0);
    assert_eq!(runners["local"]["nodeDetails"], json!([]));
    let human = isolated_command(&home)
        .env("JERYU_API_URL", &url)
        .env("JERYU_TOKEN", token)
        .args(["runners", "status"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert_eq!(
        String::from_utf8(human.stdout).unwrap().trim(),
        "runner fabric: state=unknown online=0 offline=0 activeSlots=0"
    );
    let repo = cli(
        &home,
        &url,
        token,
        &["forge", "repo", "create", "durable-repo"],
    );
    assert_eq!(repo["name"], "durable-repo");
    let source = temp.path().join("git-client");
    std::fs::create_dir(&source).unwrap();
    git(&source, token, &["init", "-b", "topic"]);
    git(
        &source,
        token,
        &["config", "user.name", "Standalone Fixture"],
    );
    git(
        &source,
        token,
        &["config", "user.email", "fixture@example.invalid"],
    );
    std::fs::write(source.join("README.md"), "durable Git fixture\n").unwrap();
    git(&source, token, &["add", "README.md"]);
    git(&source, token, &["commit", "-m", "Initial fixture"]);
    let git_url = format!("{url}/git/jeryu-admin/durable-repo.git");
    git(&source, token, &["push", &git_url, "topic"]);
    let clone = temp.path().join("clone");
    git(
        temp.path(),
        token,
        &[
            "clone",
            "--branch",
            "topic",
            &git_url,
            clone.to_str().unwrap(),
        ],
    );
    assert_eq!(
        std::fs::read_to_string(clone.join("README.md")).unwrap(),
        "durable Git fixture\n"
    );
    cli(
        &home,
        &url,
        token,
        &[
            "forge",
            "issue",
            "create",
            "--repo",
            "durable-repo",
            "--title",
            "survives restart",
        ],
    );
    assert!(data.join("forge.sqlite").is_file());
    assert!(!home.join("forge.sqlite").exists());
    let users: Value = http
        .get(format!("{url}/api/v1/admin/users"))
        .bearer_auth(token)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let users = users.as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["login"], "jeryu-admin");
    drop(server);
    let _server = start(&home, &data, &address, false);
    let runners = cli(&home, &url, token, &["runners", "status"]);
    assert_eq!(runners["local"]["state"], "unknown");
    assert_eq!(runners["local"]["totalSlots"], 0);
    let issues = cli(
        &home,
        &url,
        token,
        &["forge", "issue", "list", "--repo", "durable-repo"],
    );
    assert_eq!(issues[0]["title"], "survives restart");
}

#[test]
fn unreachable_api_and_unimplemented_operations_cannot_report_success() {
    let temp = private_temp_dir();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let result = isolated_command(temp.path())
        .args(["--api-url", &url, "forge", "repo", "list"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    let result = isolated_command(temp.path())
        .args(["cache", "self-test"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
}

#[test]
fn store_selectors_use_sqlite_and_preserve_cli_state_across_restarts() {
    let temp = private_temp_dir();
    let home = temp.path().join("home");
    let data = temp.path().join("durable");
    std::fs::create_dir(&home).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    drop(listener);
    let url = format!("http://{address}");
    let http = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let mut token = None;
    // Each process reopens the preceding process's database. The final default
    // invocation verifies the last Redline alias write survived too.
    let cases = [
        (None, None, false),
        (None, Some("sqlite"), false),
        (Some("sqlite"), Some("redline"), false),
        (Some("sqlite"), Some("unknown-env-store"), false),
        (Some("redline"), Some("sqlite"), true),
        (None, Some("redlinedb"), true),
        (Some("redlinedb"), None, true),
        (None, None, false),
    ];
    for (index, (flag, environment, notice_expected)) in cases.into_iter().enumerate() {
        let log_path = temp.path().join(format!("serve-{index}.stderr"));
        let log = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&log_path)
            .unwrap();
        let mut command = server_command(&home, &data, &address, index == 0);
        command.stderr(log);
        if let Some(flag) = flag {
            command.args(["--store", flag]);
        }
        if let Some(environment) = environment {
            command.env("JERYU_STORE", environment);
        }
        let server = wait_until_ready(command, &address);
        let mut header = [0; 16];
        std::fs::File::open(data.join("forge.sqlite"))
            .unwrap()
            .read_exact(&mut header)
            .unwrap();
        assert_eq!(&header, b"SQLite format 3\0");
        let stderr = std::fs::read_to_string(&log_path).unwrap();
        let notices: Vec<_> = stderr
            .lines()
            .filter(|line| line.contains("store=redline"))
            .collect();
        assert_eq!(
            notices.len(),
            usize::from(notice_expected),
            "flag={flag:?}, env={environment:?}"
        );
        if notice_expected {
            assert!(notices[0].contains("bundled SQLite"));
            assert!(notices[0].contains("continues"));
        }
        if index == 0 {
            token = Some(account_token(&http, &url, "jeryu-admin", false));
            let repo = cli(
                &home,
                &url,
                token.as_deref().unwrap(),
                &["forge", "repo", "create", "store-proof"],
            );
            assert_eq!(repo["name"], "store-proof");
        }
        let token = token.as_deref().unwrap();
        let issues = cli(
            &home,
            &url,
            token,
            &["forge", "issue", "list", "--repo", "store-proof"],
        );
        let mut titles: Vec<_> = issues
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| issue["title"].as_str().unwrap().to_string())
            .collect();
        titles.sort();
        assert_eq!(
            titles,
            (0..index)
                .map(|previous| format!("store restart {previous}"))
                .collect::<Vec<_>>()
        );
        if index + 1 < cases.len() {
            let title = format!("store restart {index}");
            let issue = cli(
                &home,
                &url,
                token,
                &[
                    "forge",
                    "issue",
                    "create",
                    "--repo",
                    "store-proof",
                    "--title",
                    &title,
                ],
            );
            assert_eq!(issue["title"], title);
        }
        assert!(!home.join("forge.sqlite").exists());
        drop(server);
    }
}

#[test]
fn unknown_store_exits_before_creating_any_state() {
    let temp = private_temp_dir();
    for (index, (flag, environment)) in [
        (Some("postgres"), None),
        (None, Some("postgres")),
        (Some("postgres"), Some("sqlite")),
    ]
    .into_iter()
    .enumerate()
    {
        let home = temp.path().join(format!("home-{index}"));
        std::fs::create_dir(&home).unwrap();
        let stdout_path = temp.path().join(format!("invalid-{index}.stdout"));
        let stderr_path = temp.path().join(format!("invalid-{index}.stderr"));
        let stdout = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&stdout_path)
            .unwrap();
        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&stderr_path)
            .unwrap();
        let mut command = isolated_command(&home);
        command
            .args(["serve", "--bind", "127.0.0.1:0", "--data-dir"])
            .arg(home.join("explicit-data"))
            .env("JERYU_DATA_DIR", home.join("env-data"))
            .env("XDG_DATA_HOME", home.join("xdg-data"))
            .stdout(stdout)
            .stderr(stderr);
        if let Some(flag) = flag {
            command.args(["--store", flag]);
        }
        if let Some(environment) = environment {
            command.env("JERYU_STORE", environment);
        }
        let mut child = Server(command.spawn().unwrap());
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "invalid store started a long-lived process"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(status.code(), Some(1));
        assert!(std::fs::read(stdout_path).unwrap().is_empty());
        let stderr = std::fs::read_to_string(stderr_path).unwrap();
        assert!(stderr.contains("unknown store"));
        assert!(stderr.contains("postgres"));
        assert!(!stderr.contains("store=redline"));
        assert_eq!(
            std::fs::read_dir(&home).unwrap().count(),
            0,
            "store rejection created state"
        );
    }
}
