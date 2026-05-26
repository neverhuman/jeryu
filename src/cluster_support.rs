//! Owner: Cluster Configuration Store
//! Proof: `cargo test -p jeryu -- cluster_support`
//! Invariants: Cluster configs are stored at ~/.jeryu/clusters/<alias>.toml.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::cluster_types::ClusterConfig;
use crate::install::expand_tilde;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

pub fn clusters_root() -> PathBuf {
    expand_tilde("~/.jeryu/clusters")
}

pub fn cluster_config_path(alias: &str) -> PathBuf {
    clusters_root().join(format!("{alias}.toml"))
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub fn load_cluster_config(alias: &str) -> Result<ClusterConfig> {
    let path = cluster_config_path(alias);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("loading cluster config {}", path.display()))?;
    let cfg = toml::from_str(&text)
        .with_context(|| format!("parsing cluster config {}", path.display()))?;
    Ok(cfg)
}

pub fn save_cluster_config(cfg: &ClusterConfig) -> Result<()> {
    let path = cluster_config_path(&cfg.alias);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serializing cluster config")?;
    fs::write(&path, text)
        .with_context(|| format!("writing cluster config {}", path.display()))?;
    Ok(())
}

pub fn delete_cluster_config(alias: &str) -> Result<()> {
    let path = cluster_config_path(alias);
    fs::remove_file(&path)
        .with_context(|| format!("deleting cluster config {}", path.display()))?;
    Ok(())
}

pub fn list_cluster_configs() -> Result<Vec<ClusterConfig>> {
    let root = clusters_root();
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut configs = Vec::new();
    for entry in fs::read_dir(&root)
        .with_context(|| format!("reading cluster configs dir {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        match toml::from_str::<ClusterConfig>(&text) {
            Ok(cfg) => configs.push(cfg),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping malformed cluster config");
            }
        }
    }
    configs.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(configs)
}

pub fn cluster_config_exists(alias: &str) -> bool {
    cluster_config_path(alias).exists()
}
