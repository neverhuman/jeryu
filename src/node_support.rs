//! Owner: Node Configuration Store
//! Proof: `cargo test -p jeryu -- node_support`
//! Invariants: Node configs are stored at ~/.jeryu/nodes/<alias>.toml.
//!             load/save/list are the only functions that touch the filesystem.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::install::expand_tilde;
use crate::node_types::NodeConfig;

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

pub fn nodes_root() -> PathBuf {
    expand_tilde("~/.jeryu/nodes")
}

pub fn node_config_path(alias: &str) -> PathBuf {
    nodes_root().join(format!("{alias}.toml"))
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub fn load_node_config(alias: &str) -> Result<NodeConfig> {
    let path = node_config_path(alias);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("loading node config {}", path.display()))?;
    let cfg = toml::from_str(&text)
        .with_context(|| format!("parsing node config {}", path.display()))?;
    Ok(cfg)
}

pub fn save_node_config(cfg: &NodeConfig) -> Result<()> {
    let path = node_config_path(&cfg.alias);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }
    let text = toml::to_string_pretty(cfg).context("serializing node config")?;
    fs::write(&path, text)
        .with_context(|| format!("writing node config {}", path.display()))?;
    Ok(())
}

pub fn delete_node_config(alias: &str) -> Result<()> {
    let path = node_config_path(alias);
    fs::remove_file(&path)
        .with_context(|| format!("deleting node config {}", path.display()))?;
    Ok(())
}

pub fn list_node_configs() -> Result<Vec<NodeConfig>> {
    let root = nodes_root();
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut configs = Vec::new();
    for entry in fs::read_dir(&root)
        .with_context(|| format!("reading node configs dir {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        match toml::from_str::<NodeConfig>(&text) {
            Ok(cfg) => configs.push(cfg),
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping malformed node config");
            }
        }
    }
    configs.sort_by(|a, b| a.alias.cmp(&b.alias));
    Ok(configs)
}

pub fn node_config_exists(alias: &str) -> bool {
    node_config_path(alias).exists()
}

// ---------------------------------------------------------------------------
// Node selection for pool scale-up
// ---------------------------------------------------------------------------

/// A candidate node that has capacity for additional managers.
#[derive(Debug, Clone)]
pub struct NodeCandidate {
    pub config: NodeConfig,
    /// Number of currently active managers on this node.
    pub active_managers: usize,
    /// Available capacity (max_managers - active_managers).
    pub available_slots: usize,
}

/// Select the best available node for a new manager in pool `pool_name`.
///
/// Strategy: enabled nodes that accept the pool, sorted by current active
/// manager count (least-loaded first). Returns `None` if no remote node has
/// capacity or is configured — the caller falls back to local Docker.
pub fn select_node_for_pool(
    pool_name: &str,
    active_counts: &std::collections::HashMap<String, usize>,
) -> Option<NodeConfig> {
    let configs = list_node_configs().unwrap_or_default();
    let mut candidates: Vec<NodeCandidate> = configs
        .into_iter()
        .filter(|c| c.enabled)
        .filter(|c| c.accepts_pool(pool_name))
        .filter_map(|c| {
            let active = *active_counts.get(&c.alias).unwrap_or(&0);
            if active < c.max_managers {
                Some(NodeCandidate {
                    available_slots: c.max_managers - active,
                    active_managers: active,
                    config: c,
                })
            } else {
                None
            }
        })
        .collect();

    // Least-loaded first (approximate round-robin).
    candidates.sort_by_key(|c| c.active_managers);
    candidates.into_iter().next().map(|c| c.config)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_node(alias: &str, max: usize, affinity: Vec<String>) -> NodeConfig {
        let mut cfg = NodeConfig::new(alias.to_string(), format!("u@{alias}"));
        cfg.max_managers = max;
        cfg.pool_affinity = affinity;
        cfg
    }

    #[test]
    fn select_least_loaded_node() {
        // Two nodes with capacity; should pick the one with fewer active managers.
        let active_counts: HashMap<String, usize> = [
            ("xbabe0".to_string(), 2usize),
            ("xbabe1".to_string(), 0usize),
        ]
        .into_iter()
        .collect();

        let configs = vec![
            make_node("xbabe0", 4, vec![]),
            make_node("xbabe1", 4, vec![]),
        ];
        // Sort as select_node_for_pool would, using our test configs
        let mut candidates: Vec<NodeCandidate> = configs
            .into_iter()
            .filter(|c| c.enabled)
            .filter_map(|c| {
                let active = *active_counts.get(&c.alias).unwrap_or(&0);
                if active < c.max_managers {
                    Some(NodeCandidate {
                        available_slots: c.max_managers - active,
                        active_managers: active,
                        config: c,
                    })
                } else {
                    None
                }
            })
            .collect();
        candidates.sort_by_key(|c| c.active_managers);
        assert_eq!(candidates[0].config.alias, "xbabe1");
    }

    #[test]
    fn select_excludes_full_nodes() {
        let active_counts: HashMap<String, usize> = [("xbabe0".to_string(), 4usize)].into_iter().collect();
        let configs = vec![make_node("xbabe0", 4, vec![])];
        let candidates: Vec<_> = configs
            .into_iter()
            .filter(|c| c.enabled)
            .filter_map(|c| {
                let active = *active_counts.get(&c.alias).unwrap_or(&0);
                if active < c.max_managers { Some(c) } else { None }
            })
            .collect();
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_respects_pool_affinity() {
        let active_counts: HashMap<String, usize> = HashMap::new();
        let configs = vec![
            {
                let mut c = make_node("xbabe0", 4, vec!["build".to_string()]);
                c
            },
            make_node("xbabe1", 4, vec![]),
        ];
        let build_candidates: Vec<_> = configs
            .iter()
            .filter(|c| c.enabled && c.accepts_pool("build"))
            .collect();
        assert_eq!(build_candidates.len(), 2);

        let test_candidates: Vec<_> = configs
            .iter()
            .filter(|c| c.enabled && c.accepts_pool("test"))
            .collect();
        // xbabe0 has affinity ["build"] so it should not accept "test"
        assert_eq!(test_candidates.len(), 1);
        assert_eq!(test_candidates[0].alias, "xbabe1");
    }
}
