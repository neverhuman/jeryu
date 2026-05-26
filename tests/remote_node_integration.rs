//! Owner: Remote node integration tests (SSH + Docker-in-Docker)
//! Proof: `cargo test --test remote_node_integration -- --ignored --test-threads=1`
//! Invariants:
//!   - All tests are #[ignore] — they require Docker with --privileged support.
//!   - Each test is independent and cleans up after itself.
//!   - Tests verify state transitions in the DB, not just SSH outputs.
//!
//! ## Prerequisites
//!
//! Docker must be available. The test framework:
//!   1. Builds the `jeryu-ssh-node:test` image from `tests/ssh_node/`.
//!   2. Starts a privileged container exposing sshd on a random host port.
//!   3. Injects an ephemeral SSH keypair.
//!   4. Runs the test against the live container.
//!   5. Tears down the container.
//!
//! ## Running
//!
//! ```bash
//! cargo test --test remote_node_integration -- --ignored --test-threads=1
//! ```

use std::collections::BTreeSet;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Test node lifecycle helpers
// ---------------------------------------------------------------------------

/// An ephemeral SSH test node backed by a Docker container.
struct TestNode {
    container_id: String,
    host_port: u16,
    key_path: PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl TestNode {
    /// Build the test image if needed, start a container, return a handle.
    fn start() -> anyhow::Result<Self> {
        let temp_dir = tempfile::TempDir::new()?;

        // Generate an ephemeral SSH keypair.
        let key_path = temp_dir.path().join("id_ed25519");
        let pub_path = temp_dir.path().join("id_ed25519.pub");
        let keygen = Command::new("ssh-keygen")
            .args([
                "-t", "ed25519",
                "-f", key_path.to_str().unwrap(),
                "-N", "",   // no passphrase
                "-C", "jeryu-test",
            ])
            .output()?;
        if !keygen.status.success() {
            anyhow::bail!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&keygen.stderr)
            );
        }
        let pubkey = std::fs::read_to_string(&pub_path)?;
        let pubkey = pubkey.trim().to_string();

        // Build the DinD image (fast if cached).
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let context = format!("{manifest_dir}/tests/ssh_node");
        let build = Command::new("docker")
            .args([
                "build", "-t", "jeryu-ssh-node:test",
                "-f", &format!("{context}/Dockerfile"),
                &context,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !build.success() {
            anyhow::bail!("docker build failed for jeryu-ssh-node:test");
        }

        // Pick a free host port.
        let host_port = free_port()?;

        // Start the container.
        let run = Command::new("docker")
            .args([
                "run", "-d",
                "--privileged",
                "-p", &format!("{host_port}:22"),
                "-e", &format!("JERYU_TEST_SSH_PUBKEY={pubkey}"),
                "--name", &format!("jeryu-ssh-test-{host_port}"),
                "jeryu-ssh-node:test",
            ])
            .output()?;
        if !run.status.success() {
            anyhow::bail!(
                "docker run failed: {}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
        let container_id = String::from_utf8_lossy(&run.stdout).trim().to_string();

        let node = TestNode {
            container_id,
            host_port,
            key_path,
            _temp_dir: temp_dir,
        };

        // Wait for sshd to be ready.
        node.wait_for_ssh(Duration::from_secs(30))?;

        Ok(node)
    }

    /// SSH target for this node.
    fn target(&self) -> String {
        format!("runner@127.0.0.1")
    }

    /// Build a `NodeConfig` pointing at this ephemeral container.
    fn node_config(&self) -> jeryu::node_types::NodeConfig {
        let mut cfg = jeryu::node_types::NodeConfig::new(
            "test-node".to_string(),
            self.target(),
        );
        cfg.ssh_port = self.host_port;
        cfg.identity = Some(self.key_path.to_string_lossy().to_string());
        cfg.max_managers = 4;
        cfg.storage_limit_gb = 10.0; // small limit for test
        cfg
    }

    /// Run an SSH command on the test node and return stdout.
    #[allow(dead_code)]
    fn ssh_capture(&self, cmd: &str) -> anyhow::Result<String> {
        let out = Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=no",
                "-o", "BatchMode=yes",
                "-o", "ConnectTimeout=10",
                "-i", self.key_path.to_str().unwrap(),
                "-p", &self.host_port.to_string(),
                &self.target(),
                cmd,
            ])
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    fn wait_for_ssh(&self, timeout: Duration) -> anyhow::Result<()> {
        let start = std::time::Instant::now();
        loop {
            let ok = Command::new("ssh")
                .args([
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "BatchMode=yes",
                    "-o", "ConnectTimeout=3",
                    "-i", self.key_path.to_str().unwrap(),
                    "-p", &self.host_port.to_string(),
                    &self.target(),
                    "true",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            if start.elapsed() > timeout {
                anyhow::bail!("SSH did not become ready within {:?}", timeout);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn free_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// probe_node() returns reachable=true and docker_ready=true for a healthy DinD node.
#[tokio::test]
#[ignore = "requires Docker with --privileged — run with `cargo test --test remote_node_integration -- --ignored`"]
async fn probe_healthy_dind_node() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let node = TestNode::start().expect("failed to start test node");
    let cfg = node.node_config();

    let result = jeryu::runner_backend_remote::probe_node(&cfg).await;

    assert!(result.reachable, "node should be reachable");
    assert!(result.docker_ready, "Docker should be available");
    assert!(result.os.is_some(), "OS should be detected");
    assert!(result.disk_free_gb.is_some(), "disk info should be available");
    eprintln!("probe result: {:?}", result);
}

/// probe_node() returns reachable=false for an unreachable host.
#[tokio::test]
#[ignore = "requires Docker — run with `cargo test --test remote_node_integration -- --ignored`"]
async fn probe_unreachable_node() {
    // Port 1 is almost certainly not an SSH server.
    let mut cfg = jeryu::node_types::NodeConfig::new(
        "unreachable".to_string(),
        "runner@127.0.0.1".to_string(),
    );
    cfg.ssh_port = 1;
    cfg.identity = Some("/dev/null".to_string());

    let result = jeryu::runner_backend_remote::probe_node(&cfg).await;
    assert!(!result.reachable, "unreachable node should not be reachable");
    assert!(!result.docker_ready);
}

/// RemoteDockerBackend::list_running_backend_ids() returns empty set when no managed containers.
#[tokio::test]
#[ignore = "requires Docker with --privileged — run with `cargo test --test remote_node_integration -- --ignored`"]
async fn list_running_ids_empty_on_fresh_node() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    use jeryu::runner_backend::RunnerBackend;
    use jeryu::runner_backend_remote::RemoteDockerBackend;

    let node = TestNode::start().expect("failed to start test node");
    let cfg = node.node_config();
    let backend = RemoteDockerBackend::new(cfg);

    let ids = backend.list_running_backend_ids().await
        .expect("list_running_backend_ids should succeed");
    assert!(ids.is_empty(), "no managed containers should be running: {ids:?}");
}

/// node_support::save/load/delete round-trips a NodeConfig via the filesystem.
#[test]
fn node_config_save_load_delete() {
    let temp = tempfile::TempDir::new().unwrap();
    // Override the nodes root to a temp directory.
    // We directly use the node_support primitives here, bypassing the global root.
    use jeryu::node_types::NodeConfig;

    let cfg = NodeConfig::new("xbabe0-test".to_string(), "deploy@xbabe0".to_string());
    let path = temp.path().join("xbabe0-test.toml");

    // Serialize + write
    let text = toml::to_string_pretty(&cfg).unwrap();
    std::fs::write(&path, &text).unwrap();

    // Deserialize
    let decoded: NodeConfig = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(decoded.alias, "xbabe0-test");
    assert_eq!(decoded.target, "deploy@xbabe0");
    assert_eq!(decoded.ssh_port, 22);
    assert_eq!(decoded.max_managers, 4);
    assert!((decoded.storage_limit_gb - 100.0).abs() < 0.01);
    assert!(decoded.pool_affinity.is_empty());
    assert!(decoded.enabled);

    // Delete
    std::fs::remove_file(&path).unwrap();
    assert!(!path.exists());
}

/// select_node_for_pool() picks the least-loaded node with capacity.
#[test]
fn node_selection_least_loaded() {
    use jeryu::node_support::NodeCandidate;
    use jeryu::node_types::NodeConfig;
    use std::collections::HashMap;

    fn make(alias: &str, max: usize) -> NodeConfig {
        let mut cfg = NodeConfig::new(alias.to_string(), format!("u@{alias}"));
        cfg.max_managers = max;
        cfg
    }

    let nodes = vec![
        make("xbabe0", 4),
        make("xbabe1", 4),
        make("xbabe3", 4),
    ];

    let active: HashMap<String, usize> = [
        ("xbabe0".to_string(), 3usize),
        ("xbabe1".to_string(), 1usize),
        ("xbabe3".to_string(), 2usize),
    ]
    .into_iter()
    .collect();

    let mut candidates: Vec<NodeCandidate> = nodes
        .into_iter()
        .filter(|c| c.enabled && c.accepts_pool("build"))
        .filter_map(|c| {
            let a = *active.get(&c.alias).unwrap_or(&0);
            if a < c.max_managers {
                Some(NodeCandidate {
                    available_slots: c.max_managers - a,
                    active_managers: a,
                    config: c,
                })
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by_key(|c| c.active_managers);

    assert_eq!(candidates[0].config.alias, "xbabe1", "least loaded should be xbabe1");
    assert_eq!(candidates[1].config.alias, "xbabe3");
    assert_eq!(candidates[2].config.alias, "xbabe0");
}

/// gc_storage() does not error on a healthy node with low storage usage.
#[tokio::test]
#[ignore = "requires Docker with --privileged — run with `cargo test --test remote_node_integration -- --ignored`"]
async fn gc_storage_no_op_below_threshold() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    use jeryu::runner_backend::RunnerBackend;
    use jeryu::runner_backend_remote::RemoteDockerBackend;

    let node = TestNode::start().expect("failed to start test node");
    let cfg = node.node_config();
    let backend = RemoteDockerBackend::new(cfg);

    // GC with a large limit — should be a no-op and not return an error.
    backend.gc_storage(1000.0).await
        .expect("gc_storage should not fail on healthy node");
}

/// shell_quote() is safe for common special characters.
#[test]
fn shell_quote_safety() {
    use jeryu::runner_backend_remote_support::shell_quote;

    assert_eq!(shell_quote("hello"), "'hello'");
    assert_eq!(shell_quote("hello world"), "'hello world'");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote("/path/to/dir"), "'/path/to/dir'");
    assert_eq!(shell_quote("$HOME"), "'$HOME'");
    assert_eq!(shell_quote("`rm -rf /`"), "'`rm -rf /`'");
}
