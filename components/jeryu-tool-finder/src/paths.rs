//! Default path resolution: the finder runs from its repo root inside the
//! split checkout, so siblings (the family manifest, jeryu-tool) resolve
//! relative to the current directory.

use std::path::PathBuf;

/// The finder repo root (the working directory `just` runs recipes from).
pub fn repo_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// The split root holding every family repo (parent of the finder repo).
pub fn split_root() -> PathBuf {
    let root = repo_root();
    root.parent().map(PathBuf::from).unwrap_or(root)
}

/// Default family manifest: `<split root>/repos.manifest.toml`.
pub fn default_manifest() -> PathBuf {
    split_root().join("repos.manifest.toml")
}

/// Default codegraph database, honoring `JERYU_CODEGRAPH_DB`.
pub fn default_db() -> PathBuf {
    std::env::var_os("JERYU_CODEGRAPH_DB")
        .map(PathBuf::from)
        .unwrap_or_else(jeryu_codegraph::default_db_path)
}

/// Default clusters output: `dossiers/clusters.json` in the finder repo.
pub fn default_clusters_out() -> PathBuf {
    repo_root().join("dossiers").join("clusters.json")
}

/// Default dossier output directory.
pub fn default_dossier_dir() -> PathBuf {
    repo_root().join("dossiers")
}

/// The sibling jeryu-tool checkout (registry owner).
pub fn default_jeryu_tool() -> PathBuf {
    split_root().join("jeryu-tool")
}
