//! DB driver boundary gate.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::report::GateReport;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".jankurai",
    ".worktrees",
    "dist",
    "node_modules",
    "playwright-report",
    "storybook-static",
    "target",
];

#[derive(Debug, Deserialize)]
struct Boundaries {
    db: DbBoundary,
}

#[derive(Debug, Deserialize)]
struct DbBoundary {
    truth_owner: String,
}

/// Enforces that direct SQLite driver references stay in the configured DB
/// truth owner crate.
pub fn db_boundary(root: &Path, boundaries_toml: &Path) -> Result<GateReport> {
    let raw = std::fs::read_to_string(boundaries_toml)
        .with_context(|| format!("reading {}", boundaries_toml.display()))?;
    let boundaries: Boundaries = toml::from_str(&raw)
        .with_context(|| format!("parsing {} as TOML", boundaries_toml.display()))?;

    let allowed_prefix = PathBuf::from("crates").join(boundaries.db.truth_owner);
    let pattern = ["rus", "qlite"].concat();
    let mut hits = Vec::new();
    for (rel, path) in iter_files(root)? {
        if rel == Path::new("Cargo.lock") || rel.starts_with(&allowed_prefix) {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        if text.contains(&pattern) {
            hits.push(rel.to_string_lossy().into_owned());
        }
    }

    if hits.is_empty() {
        Ok(GateReport::pass("db boundary ok"))
    } else {
        let mut lines = vec!["sqlite-driver boundary violations:".to_string()];
        lines.extend(hits.into_iter().map(|hit| format!("  {hit}")));
        Ok(GateReport::fail(lines))
    }
}

fn iter_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?;
        for entry in entries {
            let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .with_context(|| format!("reading metadata for {}", path.display()))?;
            let rel = path
                .strip_prefix(root)
                .with_context(|| format!("computing relative path for {}", path.display()))?;

            if rel.components().any(|component| {
                SKIP_DIRS.contains(&component.as_os_str().to_string_lossy().as_ref())
            }) {
                continue;
            }

            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                out.push((rel.to_path_buf(), path));
            }
        }
    }
    Ok(out)
}
