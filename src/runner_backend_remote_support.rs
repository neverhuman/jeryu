//! Owner: Remote Docker Backend — SSH helpers, probe, and storage GC
//! Proof: `cargo test -p jeryu -- runner_backend_remote_support`
//! Invariants:
//!   - `shell_quote` is safe for all strings that do not contain NUL bytes.
//!   - `probe_node` never panics; SSH failures return `reachable: false`.
//!   - `gc_orphaned_runner_dirs` never removes dirs for active manager IDs.

use anyhow::Result;
use std::collections::BTreeSet;
use tracing::info;

use crate::node_types::NodeConfig;
use crate::remote::{run_remote_shell, run_remote_shell_capture, run_remote_shell_status};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of probing a remote node's SSH + Docker availability.
#[derive(Debug, Clone, Default)]
pub struct NodeProbeResult {
    pub reachable: bool,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub docker_ready: bool,
    pub disk_free_gb: Option<f64>,
}

// ---------------------------------------------------------------------------
// Probe & GC
// ---------------------------------------------------------------------------

/// Probe basic connectivity and Docker availability on a node.
/// Each SSH step is independent — failures set the corresponding field to
/// `None` / `false` rather than propagating an error.
pub async fn probe_node(node: &NodeConfig) -> NodeProbeResult {
    let cfg = node.as_remote_config();

    // OS + arch: best-effort SSH probe.
    let os_output = match run_remote_shell_capture(&cfg, "uname -sm 2>/dev/null").await {
        Ok(Some(out)) => out,
        _ => String::new(),
    };
    let mut parts = os_output.trim().splitn(2, ' ');
    let os = parts.next().map(str::to_string).filter(|s| !s.is_empty());
    let arch = parts.next().map(str::to_string).filter(|s| !s.is_empty());

    // Docker: returns false if SSH or docker info fails.
    let docker_ready = run_remote_shell_status(&cfg, "docker info >/dev/null 2>&1")
        .await
        .unwrap_or(false);

    // Free disk (in GiB): best-effort, None on SSH or parse failure.
    let disk_free_gb =
        match run_remote_shell_capture(&cfg, "df -Pk $HOME 2>/dev/null | awk 'NR==2 {print $4}'")
            .await
        {
            Ok(Some(out)) => out
                .trim()
                .parse::<u64>()
                .ok()
                .map(|kb| kb as f64 / (1024.0 * 1024.0)),
            _ => None,
        };

    NodeProbeResult {
        reachable: os.is_some() || docker_ready,
        os,
        arch,
        docker_ready,
        disk_free_gb,
    }
}

/// Clean up runner config directories for managers that are no longer active.
/// `active_manager_ids` are manager UUIDs currently in `starting|online|node_unreachable` state.
pub async fn gc_orphaned_runner_dirs(
    node: &NodeConfig,
    active_manager_ids: &BTreeSet<String>,
) -> Result<()> {
    let cfg = node.as_remote_config();

    let script = format!(
        "ls -1 {} 2>/dev/null || true",
        shell_quote(&node.runner_data_dir)
    );
    let output = run_remote_shell_capture(&cfg, &script)
        .await?
        .unwrap_or_default();

    for entry in output.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if !active_manager_ids.contains(entry) {
            let dir = format!("{}/{}", node.runner_data_dir, entry);
            let rm = format!("rm -rf {} 2>/dev/null || true", shell_quote(&dir));
            match run_remote_shell(&cfg, &rm, true).await {
                Ok(()) => {
                    info!(node = %node.alias, dir, "removed orphaned runner config directory")
                }
                Err(e) => {
                    tracing::debug!(node = %node.alias, dir, error = %e, "orphaned dir removal skipped")
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SSH shell helpers
// ---------------------------------------------------------------------------

/// Single-quote a shell argument for remote execution.
/// Safe for all strings that do not contain NUL bytes.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Base64-encode bytes for safe transfer via shell heredoc.
pub fn base64_encode(data: &[u8]) -> String {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() {
            data[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < data.len() {
            data[i + 2] as u32
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        encoded.push(alphabet[((triple >> 18) & 0x3F) as usize] as char);
        encoded.push(alphabet[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < data.len() {
            encoded.push(alphabet[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            encoded.push('=');
        }
        if i + 2 < data.len() {
            encoded.push(alphabet[(triple & 0x3F) as usize] as char);
        } else {
            encoded.push('=');
        }
        i += 3;
    }
    encoded
}

/// Bootstrap command for a docker-executor runner container on a remote node.
/// Installs sccache if missing, then starts gitlab-runner.
pub fn runner_bootstrap_cmd_docker() -> String {
    let ver = &crate::settings::get().sccache.binary_version;
    format!(
        r#"set -eu
if ! command -v sccache >/dev/null 2>&1; then
  curl -fsSL https://github.com/mozilla/sccache/releases/download/{ver}/sccache-{ver}-x86_64-unknown-linux-musl.tar.gz \
    | tar -xz --strip-components=1 -C /usr/local/bin sccache-{ver}-x86_64-unknown-linux-musl/sccache 2>/dev/null || true
fi
exec gitlab-runner run"#
    )
}

// ---------------------------------------------------------------------------
// Private SSH I/O helper
// ---------------------------------------------------------------------------

/// Returns used kilobytes under `path` on the remote node via `du -sk`.
pub(crate) async fn get_remote_used_kb(
    cfg: &crate::remote::RemoteConfig,
    path: &str,
) -> Result<u64> {
    let script = format!(
        "du -sk {} 2>/dev/null | awk '{{print $1}}'",
        shell_quote(path)
    );
    let output = match run_remote_shell_capture(cfg, &script).await? {
        Some(out) => out,
        None => return Ok(0),
    };
    match output.trim().parse::<u64>() {
        Ok(kb) => Ok(kb),
        Err(_) => Ok(0), // SSH ran but du produced unexpected output
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_basic() {
        assert_eq!(shell_quote("hello"), "'hello'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote("/path/to/dir"), "'/path/to/dir'");
    }

    #[test]
    fn shell_quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn base64_encode_hello() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn base64_encode_empty() {
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn base64_encode_padding_two() {
        // "a" → "YQ==" (two padding chars)
        assert_eq!(base64_encode(b"a"), "YQ==");
    }

    #[test]
    fn base64_encode_padding_one() {
        // "ab" → "YWI=" (one padding char)
        assert_eq!(base64_encode(b"ab"), "YWI=");
    }

    #[test]
    fn shell_quote_roundtrip_path() {
        let path = "/home/runner/.jeryu/runners/my-manager-id";
        let quoted = shell_quote(path);
        assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
    }
}
