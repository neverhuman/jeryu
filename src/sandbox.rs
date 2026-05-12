//! Owner: Workload Sandbox (Network-Namespace Isolation)
//! Proof: `cargo test -p jeryu -- sandbox`
//! Invariants: strict_network_isolation fails closed unless an isolation backend is available;
//!             all env vars flow through SandboxConfig, never shell-escaped; do not weaken isolation without an AER

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub use_strict_network_isolation: bool,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub bind_workspace: String,
    pub extra_envs: Vec<(String, String)>,
}

pub struct ExecutorSandbox {
    config: SandboxConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackend {
    SoftLocal,
    Bubblewrap,
    Unshare,
    Unavailable,
}

impl ExecutorSandbox {
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    pub fn selected_backend(&self) -> SandboxBackend {
        if !self.config.use_strict_network_isolation {
            return SandboxBackend::SoftLocal;
        }
        detect_strict_backend()
    }

    /// Spawns the workload, optionally inside a strict network namespace
    /// using `unshare` or `bwrap` depending on the environment capabilities.
    /// Strict mode never silently downgrades to soft local execution.
    pub fn spawn_script(&self, script_path: &str) -> Result<Child> {
        let mut cmd = match self.selected_backend() {
            SandboxBackend::SoftLocal => bash_command(),
            SandboxBackend::Bubblewrap => {
                let mut c = Command::new("bwrap");
                c.arg("--die-with-parent")
                    .arg("--unshare-net")
                    .arg("--proc")
                    .arg("/proc")
                    .arg("--dev")
                    .arg("/dev")
                    .arg("--ro-bind")
                    .arg("/")
                    .arg("/")
                    .arg("--tmpfs")
                    .arg("/tmp")
                    .arg("--chdir")
                    .arg(&self.config.bind_workspace)
                    .arg("bash");
                c.env("JERYU_SANDBOXED", "strict:bwrap");
                c
            }
            SandboxBackend::Unshare => {
                let mut c = Command::new("unshare");
                c.arg("--net").arg("--").arg("bash");
                c.env("JERYU_SANDBOXED", "strict:unshare");
                c
            }
            SandboxBackend::Unavailable => {
                anyhow::bail!(
                    "strict network isolation requested but neither bwrap nor unshare is available"
                );
            }
        };

        cmd.arg(script_path);

        if !self.config.proxy_host.is_empty() && self.config.proxy_port != 0 {
            let proxy_url = format!(
                "http://{}:{}",
                self.config.proxy_host, self.config.proxy_port
            );
            cmd.env("HTTP_PROXY", &proxy_url)
                .env("HTTPS_PROXY", &proxy_url)
                .env("NO_PROXY", "localhost,127.0.0.1,::1");
        }

        for (k, v) in &self.config.extra_envs {
            cmd.env(k, v);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        cmd.spawn().context("Failed to spawn sandboxed script")
    }
}

fn bash_command() -> Command {
    if command_exists("bash") {
        Command::new("bash")
    } else if std::path::Path::new("/usr/bin/bash").is_file() {
        Command::new("/usr/bin/bash")
    } else {
        Command::new("/bin/bash")
    }
}

fn detect_strict_backend() -> SandboxBackend {
    if command_exists("bwrap") {
        SandboxBackend::Bubblewrap
    } else if command_exists("unshare") {
        SandboxBackend::Unshare
    } else {
        SandboxBackend::Unavailable
    }
}

fn command_exists(command: &str) -> bool {
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|path| path.join(command))
                .find(|path| path.is_file())
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn test_sandbox_proxy_injection() {
        let config = SandboxConfig {
            use_strict_network_isolation: false,
            proxy_host: "127.0.0.1".into(),
            proxy_port: 19800,
            bind_workspace: "/tmp".into(),
            extra_envs: vec![],
        };
        let sandbox = ExecutorSandbox::new(config);

        let mut script_file = tempfile::NamedTempFile::new().unwrap();
        script_file.write_all(b"echo $HTTP_PROXY").unwrap();

        let child = sandbox
            .spawn_script(script_file.path().to_str().unwrap())
            .unwrap();
        let output = child.wait_with_output().await.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), "http://127.0.0.1:19800");
    }

    #[test]
    fn soft_mode_reports_soft_backend() {
        let config = SandboxConfig {
            use_strict_network_isolation: false,
            proxy_host: "10.0.0.1".into(),
            proxy_port: 8080,
            bind_workspace: "/tmp".into(),
            extra_envs: vec![],
        };
        let sandbox = ExecutorSandbox::new(config);
        assert_eq!(sandbox.selected_backend(), SandboxBackend::SoftLocal);
    }

    #[tokio::test]
    async fn strict_sandbox_never_silently_downgrades_to_soft() {
        let config = SandboxConfig {
            use_strict_network_isolation: true,
            proxy_host: "10.0.0.1".into(),
            proxy_port: 8080,
            bind_workspace: "/tmp".into(),
            extra_envs: vec![],
        };
        let sandbox = ExecutorSandbox::new(config);

        let mut script_file = tempfile::NamedTempFile::new().unwrap();
        script_file.write_all(b"echo $JERYU_SANDBOXED").unwrap();

        match sandbox.selected_backend() {
            SandboxBackend::Unavailable => {
                let err = sandbox
                    .spawn_script(script_file.path().to_str().unwrap())
                    .unwrap_err()
                    .to_string();
                assert!(err.contains("strict network isolation requested"));
            }
            SandboxBackend::Bubblewrap | SandboxBackend::Unshare => {}
            SandboxBackend::SoftLocal => panic!("strict mode reported soft local backend"),
        }
    }
}
