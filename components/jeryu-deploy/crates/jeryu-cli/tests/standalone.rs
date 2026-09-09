//! Process-level standalone proof using empty configuration and durable storage.

use reqwest::{Method, blocking::Client};
use serde_json::{Value, json};
use std::{
    net::TcpListener,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::TempDir;

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

fn start(home: &Path, data: &Path, address: &str, initialize: bool) -> Server {
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
fn authenticated_protected_review_checks_merge_and_restart_preserve_exact_head() {
    let temp = TempDir::new().unwrap();
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
    let temp = TempDir::new().unwrap();
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
    let temp = TempDir::new().unwrap();
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
