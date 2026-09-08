//! Process-level standalone proof using empty configuration and durable storage.

use reqwest::blocking::Client;
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

fn git(directory: &Path, token: &str, args: &[&str]) {
    let output = Command::new("git")
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
        .unwrap();
    assert!(
        output.status.success(),
        "Git operation failed: {}",
        String::from_utf8_lossy(&output.stderr)
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
