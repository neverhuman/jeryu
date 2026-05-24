//! Owner: Interactive TUI subsystem — proof-lanes.toml loader for Mission tab
//! Proof: `cargo nextest run -p jeryu -- tui::proof_lanes`
//! Invariants: read-only; non-fatal. Missing or unparseable file yields an
//! empty lane list, falling back to the legacy hardcoded view.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProofLane {
    pub id: String,
    pub command: String,
    pub description: String,
    /// `["any"]` means runs on every change; specific tags otherwise.
    pub required_for: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProofLanesSnapshot {
    pub loaded: bool,
    pub lanes: Vec<ProofLane>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProofLanesFile {
    #[serde(default)]
    lane: std::collections::BTreeMap<String, LaneEntry>,
}

#[derive(Debug, Deserialize)]
struct LaneEntry {
    #[serde(default)]
    command: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    required_for: Vec<String>,
}

pub fn load_snapshot() -> ProofLanesSnapshot {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    load_snapshot_from(&repo_root)
}

pub(crate) fn load_snapshot_from(repo_root: &Path) -> ProofLanesSnapshot {
    let path = repo_root.join("proof-lanes.toml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return ProofLanesSnapshot::default();
        }
        Err(err) => {
            return ProofLanesSnapshot {
                error: Some(format!("{}: {}", path.display(), err)),
                ..Default::default()
            };
        }
    };
    match toml::from_str::<ProofLanesFile>(&raw) {
        Ok(file) => {
            let lanes = file
                .lane
                .into_iter()
                .map(|(id, entry)| ProofLane {
                    id,
                    command: entry.command,
                    description: entry.description,
                    required_for: entry.required_for,
                })
                .collect();
            ProofLanesSnapshot {
                loaded: true,
                lanes,
                error: None,
            }
        }
        Err(err) => ProofLanesSnapshot {
            error: Some(format!("{}: {}", path.display(), err)),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(!snap.loaded);
    }

    #[test]
    fn parses_real_shape() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("proof-lanes.toml"),
            r#"
[lane.check]
command = "cargo check"
description = "type check"
required_for = ["any"]

[lane.unit]
command = "cargo nextest run"
description = "unit tests"
required_for = ["any"]
"#,
        )
        .unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(snap.loaded);
        assert_eq!(snap.lanes.len(), 2);
        let check = snap.lanes.iter().find(|l| l.id == "check").unwrap();
        assert_eq!(check.command, "cargo check");
    }
}
