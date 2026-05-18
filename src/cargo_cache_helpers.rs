use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub(crate) fn process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let pid = pid as i32;
        if pid <= 0 {
            return false;
        }
        // SAFETY: kill(0) checks for process existence without sending a signal.
        let rc = unsafe { libc::kill(pid, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(not(unix))]
    {
        pid > 0
    }
}

pub(crate) fn current_rustc_toolchain() -> Result<crate::cargo_cache::CargoToolchainKey> {
    let output = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .context("running rustc -vV")?;
    if !output.status.success() {
        anyhow::bail!("rustc -vV failed with status {:?}", output.status.code());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let rustc_version = stdout
        .lines()
        .next()
        .unwrap_or("rustc unknown")
        .trim()
        .to_string();
    let host_triple = stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap_or("unknown-host")
        .trim()
        .to_string();
    let rustc_key = short_hash(stdout.as_bytes());
    Ok(crate::cargo_cache::CargoToolchainKey {
        rustc_key,
        rustc_version,
        host_triple,
    })
}

pub(crate) fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | ' ' | '\t' | '\n' | '\r' => '_',
            other => other,
        })
        .collect()
}

pub(crate) fn short_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())[..12].to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub(crate) fn usable_sccache_binary() -> Option<PathBuf> {
    if !crate::settings::get().sccache.enabled {
        return None;
    }

    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("sccache");
        if is_usable_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_usable_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    metadata.permissions().mode() & 0o111 != 0
}

pub fn shell_exports(layout: &crate::cargo_cache::CargoCacheLayout) -> Vec<String> {
    let mut lines = Vec::new();
    for (key, value) in &layout.env {
        lines.push(format!("export {}={}", key, shell_quote(value)));
    }
    if !layout.cargo_cache_enabled {
        lines.push("unset CARGO_TARGET_DIR".to_string());
    }
    if !layout.env.contains_key("RUSTC_WRAPPER") {
        lines.push(
            "unset RUSTC_WRAPPER SCCACHE_DIR SCCACHE_NO_DAEMON SCCACHE_CACHE_SIZE".to_string(),
        );
    }
    lines
}
