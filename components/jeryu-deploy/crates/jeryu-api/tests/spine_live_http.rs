#![cfg(feature = "web")]
//! S4 live-HTTP e2e: boot `serve()` on a real loopback socket, create a repo
//! over HTTP (which materializes a bare repo on disk), then clone, commit, push
//! an allowed branch over smart-HTTP, and prove a direct client push to
//! `refs/heads/main` is rejected. This exercises create-repo-to-disk (S3), the
//! git transport, and the main-protection receive policy on unified `jeryu serve`
//! (S4) end to end.

use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use flate2::Compression;
use flate2::write::GzEncoder;
use jeryu_api::web::{WebServerConfig, serve};
use sha2::Digest;

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn git_lfs_available() -> bool {
    Command::new("git")
        .args(["lfs", "version"])
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

fn run_git_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) {
    let mut command = Command::new("git");
    command.args(args).current_dir(dir);
    for (key, value) in envs {
        command.env(key, value);
    }
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn run_git_failure(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    assert!(
        !output.status.success(),
        "git {args:?} unexpectedly succeeded in {}",
        dir.display()
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn write_bytes(path: &Path, seed: u8, len: usize) -> Vec<u8> {
    let bytes: Vec<u8> = (0..len)
        .map(|idx| seed.wrapping_add((idx % 251) as u8))
        .collect();
    std::fs::write(path, &bytes).unwrap();
    bytes
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

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn pkt_line(payload: &[u8]) -> Vec<u8> {
    let length = payload.len() + 4;
    assert!(length <= 0xffff);
    let mut encoded = format!("{length:04x}").into_bytes();
    encoded.extend_from_slice(payload);
    encoded
}

async fn wait_until_listening(addr: SocketAddr, server: &mut tokio::task::JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    loop {
        if server.is_finished() {
            let result = server.await;
            panic!("server task exited before readiness on {addr}: {result:?}");
        }
        // Probe the backend route. An unknown SPA path can return the HTML
        // shell in a monorepo build and mask a broken readiness check.
        let health = client.get(format!("http://{addr}/health")).send().await;
        let ready = match health {
            Ok(response) if response.status().is_success() => response
                .json::<serde_json::Value>()
                .await
                .is_ok_and(|body| body["status"] == "ok" && body["service"] == "jeryu-api"),
            _ => false,
        };
        if ready {
            tokio::task::yield_now().await;
            if server.is_finished() {
                let result = server.await;
                panic!("server task exited before readiness on {addr}: {result:?}");
            }
            return;
        }
        assert!(Instant::now() < deadline, "server never listened on {addr}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "current_thread")]
#[should_panic(expected = "server task exited before readiness")]
async fn readiness_rejects_an_already_exited_server_task() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut server = tokio::spawn(async {});
    tokio::task::yield_now().await;

    wait_until_listening(addr, &mut server).await;
}

fn assert_lfs_content_type(resp: &reqwest::Response) {
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("application/vnd.git-lfs+json"),
        "expected Git LFS JSON content type, got {content_type}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s4_create_repo_to_disk_and_git_push_over_http_blocks_main() {
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
        split_manifests: Vec::new(),
        auth_required: false,
        trust_local_dev: true,
        secure_cookies: false,
    };
    let mut server = tokio::spawn(async move { serve(config).await.unwrap() });
    wait_until_listening(addr, &mut server).await;

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

    // 3. Commit and push back over the same transport to an allowed branch.
    run_git(
        &clone_dir,
        &["config", "user.email", "tester@jeryu.invalid"],
    );
    run_git(&clone_dir, &["config", "user.name", "Tester"]);
    write_incompressible_file(&clone_dir.join("hello.bin"), 3 * 1024 * 1024);
    // A GitHub-Actions workflow so the push path still carries realistic repo
    // contents; the branch itself is not the protected main ref.
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
    eprintln!("[s4] git push feature");
    run_git(
        &clone_dir,
        &[
            GIT_HTTP_GUARD,
            &["-c", "pack.compression=0", "-c", "core.compression=0"],
            &["push", "origin", "HEAD:refs/heads/feature"],
        ]
        .concat(),
    );
    eprintln!("[s4] pushed feature");

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

    // 4. Assert the allowed branch landed in the on-disk bare repo.
    let feature = Command::new("git")
        .args([
            "--git-dir",
            bare.to_str().unwrap(),
            "rev-parse",
            "refs/heads/feature",
        ])
        .output()
        .unwrap();
    assert!(
        feature.status.success(),
        "refs/heads/feature must exist in the bare repo after push: {}",
        String::from_utf8_lossy(&feature.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&feature.stdout).trim(), sha);

    // 5. The same HTTP client cannot advance protected main directly.
    eprintln!("[s4] git push main should be rejected");
    let failure = run_git_failure(
        &clone_dir,
        &[
            GIT_HTTP_GUARD,
            &["-c", "pack.compression=0", "-c", "core.compression=0"],
            &["push", "origin", "HEAD:refs/heads/main"],
        ]
        .concat(),
    );
    assert!(
        failure.contains("direct pushes to refs/heads/main are blocked")
            || failure.contains("The requested URL returned error: 403"),
        "main push should be rejected by the protected-ref policy: {failure}"
    );
    let main = Command::new("git")
        .args([
            "--git-dir",
            bare.to_str().unwrap(),
            "rev-parse",
            "--verify",
            "refs/heads/main",
        ])
        .output()
        .unwrap();
    assert!(
        !main.status.success(),
        "refs/heads/main must not be created by a rejected direct push"
    );

    server.abort();
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s4_git_pack_rpc_routes_decode_gzip_before_git() {
    let base = std::env::temp_dir().join(format!("jeryu-s4-gzip-rpc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data_dir = base.join("data");
    let git_root = base.join("git");
    let spa_dir = base.join("spa");
    std::fs::create_dir_all(&spa_dir).unwrap();

    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let config = WebServerConfig {
        bind: addr,
        spa_dir,
        data_dir,
        git_storage_root: git_root,
        split_manifests: Vec::new(),
        auth_required: false,
        trust_local_dev: true,
        secure_cookies: false,
    };
    let mut server = tokio::spawn(async move { serve(config).await.unwrap() });
    wait_until_listening(addr, &mut server).await;

    let client = reqwest::Client::new();
    let create = client
        .post(format!("http://{addr}/repos"))
        .json(&serde_json::json!({ "name": "gzip-rpc" }))
        .send()
        .await
        .expect("POST /repos");
    assert_eq!(create.status().as_u16(), 201);

    // Protocol v2 permits repeated ref-prefix arguments. This is a valid
    // request larger than Git's request-compression threshold, matching the
    // complete-ref mirror failure that exposed the adapter bug.
    let mut upload_request = pkt_line(b"command=ls-refs\n");
    upload_request.extend_from_slice(b"0001");
    upload_request.extend_from_slice(&pkt_line(b"peel\n"));
    upload_request.extend_from_slice(&pkt_line(b"symrefs\n"));
    for index in 0..64 {
        upload_request.extend_from_slice(&pkt_line(
            format!("ref-prefix refs/heads/fixture-{index:03}\n").as_bytes(),
        ));
    }
    upload_request.extend_from_slice(b"0000");
    assert!(upload_request.len() > 1_024);

    let upload = client
        .post(format!(
            "http://{addr}/git/jeryu/gzip-rpc.git/git-upload-pack"
        ))
        .header(reqwest::header::CONTENT_ENCODING, "gzip")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-git-upload-pack-request",
        )
        .header("git-protocol", "version=2")
        .body(gzip(&upload_request))
        .send()
        .await
        .expect("gzip upload-pack POST");
    assert_eq!(upload.status().as_u16(), 200);
    assert_eq!(
        upload
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-git-upload-pack-result")
    );

    // A flush-only receive-pack request is valid and side-effect free. Keeping
    // this second route-level assertion prevents either handler from silently
    // dropping the shared decoder call.
    let receive = client
        .post(format!(
            "http://{addr}/git/jeryu/gzip-rpc.git/git-receive-pack"
        ))
        .header(reqwest::header::CONTENT_ENCODING, "x-gzip")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-git-receive-pack-request",
        )
        .body(gzip(b"0000"))
        .send()
        .await
        .expect("gzip receive-pack POST");
    assert_eq!(receive.status().as_u16(), 200);
    assert_eq!(
        receive
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/x-git-receive-pack-result")
    );

    server.abort();
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s4_git_lfs_batch_and_locks_verify_routes_return_protocol_json() {
    let base = std::env::temp_dir().join(format!("jeryu-s4-lfs-routes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data_dir = base.join("data");
    let git_root = base.join("git");
    let spa_dir = base.join("spa");
    std::fs::create_dir_all(&spa_dir).unwrap();

    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let config = WebServerConfig {
        bind: addr,
        spa_dir,
        data_dir,
        git_storage_root: git_root,
        split_manifests: Vec::new(),
        auth_required: false,
        trust_local_dev: true,
        secure_cookies: false,
    };
    let mut server = tokio::spawn(async move { serve(config).await.unwrap() });
    wait_until_listening(addr, &mut server).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/repos"))
        .json(&serde_json::json!({ "name": "lfs-routes" }))
        .send()
        .await
        .expect("POST /repos");
    assert_eq!(resp.status().as_u16(), 201);

    let credential = "Basic dGVzdC11c2VyOnRlc3QtdG9rZW4=";
    let object = b"authenticated-lfs-download";
    let oid = format!("{:x}", sha2::Sha256::digest(object));
    let upload = client
        .put(format!(
            "http://{addr}/git/jeryu/lfs-routes.git/info/lfs/objects/{oid}"
        ))
        .header(reqwest::header::AUTHORIZATION, credential)
        .body(object.to_vec())
        .send()
        .await
        .expect("PUT LFS object");
    assert_eq!(upload.status().as_u16(), 200);

    let batch = client
        .post(format!(
            "http://{addr}/git/jeryu/lfs-routes.git/info/lfs/objects/batch"
        ))
        .header(reqwest::header::AUTHORIZATION, credential)
        .json(&serde_json::json!({
            "operation": "download",
            "transfers": ["basic"],
            "objects": [{ "oid": oid, "size": object.len() }]
        }))
        .send()
        .await
        .expect("POST LFS batch");
    assert_eq!(batch.status().as_u16(), 200);
    assert_lfs_content_type(&batch);
    let batch_body: serde_json::Value = batch.json().await.expect("LFS batch JSON body");
    assert_eq!(batch_body["transfer"], "basic");
    assert_eq!(batch_body["objects"].as_array().unwrap().len(), 1);
    assert_eq!(
        batch_body["objects"][0]["actions"]["download"]["header"]["Authorization"],
        credential
    );

    let locks = client
        .post(format!(
            "http://{addr}/git/jeryu/lfs-routes.git/info/lfs/locks/verify"
        ))
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("POST LFS locks verify");
    assert_eq!(locks.status().as_u16(), 200);
    assert_lfs_content_type(&locks);
    let locks_body: serde_json::Value = locks.json().await.expect("LFS locks verify JSON body");
    assert_eq!(locks_body["ours"].as_array().unwrap().len(), 0);
    assert_eq!(locks_body["theirs"].as_array().unwrap().len(), 0);

    server.abort();
    let _ = std::fs::remove_dir_all(&base);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s4_git_lfs_cpkt_versions_roundtrip_over_http() {
    if !git_available() || !git_lfs_available() {
        eprintln!("git or git-lfs unavailable; skipping s4 LFS live-HTTP e2e");
        return;
    }

    let base = std::env::temp_dir().join(format!("jeryu-s4-lfs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let data_dir = base.join("data");
    let git_root = base.join("git");
    let spa_dir = base.join("spa");
    let work = base.join("work");
    std::fs::create_dir_all(&spa_dir).unwrap();
    std::fs::create_dir_all(&work).unwrap();

    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let config = WebServerConfig {
        bind: addr,
        spa_dir,
        data_dir,
        git_storage_root: git_root.clone(),
        split_manifests: Vec::new(),
        auth_required: false,
        trust_local_dev: true,
        secure_cookies: false,
    };
    let mut server = tokio::spawn(async move { serve(config).await.unwrap() });
    wait_until_listening(addr, &mut server).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/repos"))
        .json(&serde_json::json!({ "name": "lfs-demo" }))
        .send()
        .await
        .expect("POST /repos");
    assert_eq!(resp.status().as_u16(), 201);

    let clone_url = format!("http://{addr}/git/jeryu/lfs-demo.git");
    run_git(
        &work,
        &[GIT_HTTP_GUARD, &["clone", clone_url.as_str(), "source"]].concat(),
    );
    let source = work.join("source");
    run_git(&source, &["config", "user.email", "tester@jeryu.invalid"]);
    run_git(&source, &["config", "user.name", "Tester"]);
    run_git(&source, &["lfs", "install", "--local", "--skip-repo"]);
    assert!(
        !source.join(".git/hooks/pre-push").exists(),
        "the live LFS fixture must not install or execute Git hooks"
    );
    run_git(&source, &["lfs", "track", "*.cpkt"]);

    let v1 = write_bytes(&source.join("model.cpkt"), 11, 4096);
    run_git(&source, &["add", ".gitattributes", "model.cpkt"]);
    run_git(&source, &["commit", "-m", "model v1"]);
    run_git(&source, &["lfs", "push", "origin", "HEAD"]);
    run_git(
        &source,
        &[
            GIT_HTTP_GUARD,
            &["push", "origin", "HEAD:refs/heads/feature"],
        ]
        .concat(),
    );

    let v2 = write_bytes(&source.join("model.cpkt"), 29, 6144);
    run_git(&source, &["add", "model.cpkt"]);
    run_git(&source, &["commit", "-m", "model v2"]);
    run_git(&source, &["lfs", "push", "origin", "HEAD"]);
    run_git(
        &source,
        &[
            GIT_HTTP_GUARD,
            &["push", "origin", "HEAD:refs/heads/feature"],
        ]
        .concat(),
    );

    run_git_env(
        &work,
        &[
            GIT_HTTP_GUARD,
            &[
                "clone",
                "--branch",
                "feature",
                clone_url.as_str(),
                "skip-smudge",
            ],
        ]
        .concat(),
        &[
            ("GIT_LFS_SKIP_SMUDGE", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ],
    );
    let skip = work.join("skip-smudge");
    let pointer = std::fs::read_to_string(skip.join("model.cpkt")).unwrap();
    assert!(
        pointer.contains("version https://git-lfs.github.com/spec/v1")
            && pointer.contains("oid sha256:"),
        "expected LFS pointer, got {pointer}"
    );

    // Host CI runs with GIT_CONFIG_NOSYSTEM=1 and no global LFS filters, so the
    // skip-smudge clone must install local LFS before pull can materialize payloads.
    run_git_env(
        &skip,
        &["lfs", "install", "--local", "--skip-repo"],
        &[
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ],
    );
    assert!(
        !skip.join(".git/hooks/pre-push").exists(),
        "the skip-smudge fixture must not install or execute Git hooks"
    );

    run_git(&skip, &["lfs", "pull"]);
    assert_eq!(std::fs::read(skip.join("model.cpkt")).unwrap(), v2);

    run_git(&skip, &["checkout", "HEAD~1"]);
    run_git(&skip, &["lfs", "pull"]);
    assert_eq!(std::fs::read(skip.join("model.cpkt")).unwrap(), v1);

    server.abort();
    let _ = std::fs::remove_dir_all(&base);
}
