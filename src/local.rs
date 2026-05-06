//! Owner: Local agent command wrappers
//! Proof: `cargo test -p jeryu -- local`
//! Invariants: Local Cargo commands reuse jeryu-owned caches by default and never leave expired leases on the happy path.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::process::Command;

use crate::cargo_cache;
use crate::exec::run_status_check;

fn cargo_cache_enabled() -> bool {
    std::env::var("JERYU_CARGO_CACHE")
        .ok()
        .map(|value| value.trim() != "0")
        .unwrap_or(true)
}

pub async fn run_cargo(repo: PathBuf, cargo_args: Vec<String>) -> Result<()> {
    let layout = cargo_cache::local_cargo_layout(&repo, cargo_cache_enabled())?;
    if layout.env.contains_key("SCCACHE_DIR") {
        std::fs::create_dir_all(&layout.sccache_dir)
            .with_context(|| format!("creating {}", layout.sccache_dir.display()))?;
    }
    if layout.cargo_cache_enabled {
        if let Some(parent) = layout.target_dir.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::create_dir_all(&layout.target_dir)
            .with_context(|| format!("creating {}", layout.target_dir.display()))?;
    }

    let _lease = cargo_cache::write_lease(&layout)?;

    let mut command = Command::new("cargo");
    command.current_dir(&repo).args(&cargo_args);
    command.envs(layout.env.iter());

    run_status_check(&mut command, "running cargo").await
}

pub fn cargo_env(repo: PathBuf) -> Result<cargo_cache::CargoCacheLayout> {
    let cache_enabled = cargo_cache_enabled();
    cargo_cache::local_cargo_layout(&repo, cache_enabled)
}
