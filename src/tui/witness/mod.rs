//! Owner: Interactive TUI subsystem — Witness build-graph summary loader
//! Proof: `cargo nextest run -p jeryu -- tui::witness`
//! Invariants: read-only; loads only the summary (counts), not the full graph,
//! to keep the working set small. The file is ~325 KiB and updated by the
//! `witness` build-graph tool.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct WitnessFile {
    #[serde(default)]
    generated_at: String,
    #[serde(default)]
    crates: Vec<WitnessCrate>,
}

#[derive(Debug, Deserialize)]
struct WitnessCrate {
    #[serde(default)]
    name: String,
    #[serde(default)]
    pub_items: Vec<serde::de::IgnoredAny>,
}

/// What the TUI displays — counts, not the full graph. Held on
/// `TuiStateSnapshot` so renderers can show a one-liner without reparsing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WitnessSnapshot {
    pub loaded: bool,
    pub generated_at: String,
    pub crate_count: usize,
    /// Total `pub_items` across all crates. Useful as a "public API surface"
    /// signal that often correlates with rustdoc/breaking-change risk.
    pub pub_item_count: usize,
    pub largest_crate: Option<(String, usize)>,
    pub error: Option<String>,
}

pub fn load_snapshot() -> WitnessSnapshot {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    load_snapshot_from(&repo_root)
}

pub(crate) fn load_snapshot_from(repo_root: &Path) -> WitnessSnapshot {
    let path = repo_root.join(".witness").join("witness-graph.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return WitnessSnapshot::default()
        }
        Err(err) => {
            return WitnessSnapshot {
                error: Some(format!("{}: {}", path.display(), err)),
                ..Default::default()
            };
        }
    };
    match serde_json::from_str::<WitnessFile>(&raw) {
        Ok(file) => {
            let mut largest: Option<(String, usize)> = None;
            let mut total = 0usize;
            for c in &file.crates {
                let n = c.pub_items.len();
                total += n;
                if largest.as_ref().map(|(_, count)| n > *count).unwrap_or(true) {
                    largest = Some((c.name.clone(), n));
                }
            }
            WitnessSnapshot {
                loaded: true,
                generated_at: file.generated_at,
                crate_count: file.crates.len(),
                pub_item_count: total,
                largest_crate: largest,
                error: None,
            }
        }
        Err(err) => WitnessSnapshot {
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
        assert!(snap.error.is_none());
    }

    #[test]
    fn counts_crates_and_pub_items() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".witness")).unwrap();
        fs::write(
            dir.path().join(".witness").join("witness-graph.json"),
            r#"{
                "generated_at":"x",
                "crates":[
                    {"name":"a","pub_items":[1,2,3]},
                    {"name":"b","pub_items":[1]}
                ]
            }"#,
        )
        .unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(snap.loaded);
        assert_eq!(snap.crate_count, 2);
        assert_eq!(snap.pub_item_count, 4);
        assert_eq!(snap.largest_crate.unwrap().0, "a");
    }
}
