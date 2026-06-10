#![cfg(feature = "web")]
//! S4 live-HTTP e2e: boot `serve()` on a real loopback socket, create a repo
//! over HTTP (which materializes a bare repo on disk), then clone, commit, and
//! push over the mounted smart-HTTP transport and assert the pushed commit
//! landed in the on-disk bare repo. This exercises create-repo-to-disk (S3) and
//! the git transport on the unified `jeryu serve` (S4) end to end.

use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use jeryu_api::web::{WebServerConfig, serve};

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn write_incompressible_file(path: &Path, len: usize) {
    let mut file = File::create(path).unwrap();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15 ^ (len as u64);
    let mut remaining = len;
    let mut buffer = vec![0u8; 8192];
    while remaining > 0 {
        for chunk in buffer.chunks_mut(8) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bytes = state.to_le_bytes();
            let len = chunk.len();
            chunk.copy_from_slice(&bytes[..len]);
        }
        let write_len = remaining.min(buffer.len());
        file.write_all(&buffer[..write_len]).unwrap();
        remaining -= write_len;
    }
    file.flush().unwrap();
}

/// Git http config that aborts a stalled transfer instead of hanging.
const GIT_HTTP_GUARD: &[&str] = &["-c", "http.lowSpeedLimit=100", "-c", "http.lowSpeedTime=20"];

async fn wait_until_listening(addr: SocketAddr) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "server never listened on {addr}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s4_create_repo_to_disk_and_git_push_over_http() {
    if !git_available() {
        eprintln!("git unavailable; skipping s4 live-HTTP e2e");
        return;
    }

    let base = std::env::temp_dir().join(format!("jeryu-s4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data_dir = base.join("data");
    let git_root = base.join("git");
    let spa_dir = base.join("spa");
    let work = base.join("work");
    std::fs::create_dir_all(&spa_dir).unwrap();
    std::fs::create_dir_all(&work).unwrap();

    // Reserve a free loopback port, then release it for serve() to bind.
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let config = WebServerConfig {
        bind: addr,
        spa_dir,
        data_dir,
        git_storage_root: git_root.clone(),
    };
    let server = tokio::spawn(async move { serve(config).await.unwrap() });
    wait_until_listening(addr).await;

    // 1. Create the repo over HTTP -> materializes a bare repo on disk.
    eprintln!("[s4] POST /repos ...");
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/repos"))
        .json(&serde_json::json!({ "name": "demo" }))
        .send()
        .await
        .expect("POST /repos");
    let status = resp.status().as_u16();
    eprintln!("[s4] POST /repos -> {status}");
    assert_eq!(status, 201, "POST /repos should return 201");
    let bare = git_root.join("jeryu").join("demo.git");
    assert!(
        bare.join("HEAD").is_file(),
        "bare repo must exist on disk after create"
    );
    eprintln!("[s4] bare repo materialized on disk");

    // 2. Clone over HTTP (loopback-permissive: no credentials required).
    let clone_url = format!("http://{addr}/git/jeryu/demo.git");
    eprintln!("[s4] git clone {clone_url}");
    run_git(
        &work,
        &[GIT_HTTP_GUARD, &["clone", clone_url.as_str(), "clone"]].concat(),
    );
    let clone_dir = work.join("clone");
    eprintln!("[s4] cloned");

    // 3. Commit and push back over the same transport.
    run_git(
        &clone_dir,
        &["config", "user.email", "tester@jeryu.invalid"],
    );
    run_git(&clone_dir, &["config", "user.name", "Tester"]);
    write_incompressible_file(&clone_dir.join("hello.bin"), 3 * 1024 * 1024);
    // A GitHub-Actions workflow so the push->CI bridge has something to compile.
    std::fs::create_dir_all(clone_dir.join(".github/workflows")).unwrap();
    std::fs::write(
        clone_dir.join(".github/workflows/ci.yml"),
        format!(
            "name: ci\non: [push]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - run: |\n          test -d .git\n          test \"$(git remote get-url origin)\" = \"{clone_url}\"\n          git rev-parse --verify origin/main\n          test \"$(git rev-parse HEAD)\" = \"$JERYU_COMMIT_SHA\"\n          test \"$(git rev-parse origin/main)\" = \"$JERYU_COMMIT_SHA\"\n          test \"$JERYU_NETWORK_POLICY\" = \"egress-only\"\n          test \"$JERYU_SECRETS\" = \"disabled\"\n          test -z \"${{GITHUB_TOKEN:-}}\"\n",
        ),
    )
    .unwrap();
    run_git(&clone_dir, &["add", "."]);
    run_git(&clone_dir, &["commit", "-m", "first commit"]);
    eprintln!("[s4] git push");
    run_git(
        &clone_dir,
        &[
            GIT_HTTP_GUARD,
            &["-c", "pack.compression=0", "-c", "core.compression=0"],
            &["push", "origin", "HEAD:refs/heads/main"],
        ]
        .concat(),
    );
    eprintln!("[s4] pushed");

    // 4. Assert the pushed commit is in the on-disk bare repo.
    let out = Command::new("git")
        .args([
            "--git-dir",
            bare.to_str().unwrap(),
            "rev-parse",
            "refs/heads/main",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "refs/heads/main must exist in the bare repo after push: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 5. The push->CI bridge compiled the workflow and registered check-runs for
    //    the pushed commit (the receive-pack response is held until it finishes).
    let sha = String::from_utf8(
        Command::new("git")
            .args(["-C", clone_dir.to_str().unwrap(), "rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    let runs = loop {
        let runs: serde_json::Value = client
            .get(format!(
                "http://{addr}/repos/jeryu/demo/commits/{sha}/check-runs"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if runs
            .get("total_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            >= 1
        {
            break runs;
        }
        assert!(
            Instant::now() < deadline,
            "push->CI bridge should register a check-run before the deadline, got {runs}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let total = runs
        .get("total_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let conclusion = runs["check_runs"][0]["conclusion"].as_str().unwrap_or("");
    assert_eq!(
        conclusion, "success",
        "the bridge executed the workflow job in the sandbox; it should be green: {runs}"
    );
    eprintln!("[s4] push->CI executed job in sandbox -> {conclusion} ({total} check-run)");

    server.abort();
    let _ = std::fs::remove_dir_all(&base);
}
