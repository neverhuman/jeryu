//! Owner: Interactive TUI subsystem — VRC (Verification, Reproducibility, Coverage) plan loader
//! Proof: `cargo nextest run -p jeryu -- tui::vrc`
//! Invariants: read-only; non-fatal; the minimal `{mode, reason}` shape is the
//! ground floor — richer fields from `cargo vrc plan` populate when available.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct VrcPlanFile {
    /// `"full"` (no changed files detected) or `"selected"` (VTI selection applied)
    /// or `"none"` (nothing to run).
    #[serde(default)]
    pub mode: String,
    /// Human-readable explanation of why the planner picked `mode`.
    #[serde(default)]
    pub reason: String,
    /// Optional richer fields emitted by `cargo vrc plan` when there is
    /// VTI selection signal. The TUI banner falls back to mode+reason
    /// when these are absent.
    #[serde(default)]
    pub selected_tests: Vec<String>,
    #[serde(default)]
    pub skipped_tests: Vec<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VrcSnapshot {
    /// `true` once the file has been read at least once. Distinguishes
    /// "no plan generated" from "loading".
    pub loaded: bool,
    pub plan: VrcPlanFile,
    pub error: Option<String>,
}

pub fn load_snapshot() -> VrcSnapshot {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    load_snapshot_from(&repo_root)
}

pub(crate) fn load_snapshot_from(repo_root: &Path) -> VrcSnapshot {
    let path = repo_root.join("vrc-plan.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return VrcSnapshot::default(),
        Err(err) => {
            return VrcSnapshot {
                loaded: false,
                plan: VrcPlanFile::default(),
                error: Some(format!("{}: {}", path.display(), err)),
            };
        }
    };
    match serde_json::from_str::<VrcPlanFile>(&raw) {
        Ok(plan) => VrcSnapshot {
            loaded: true,
            plan,
            error: None,
        },
        Err(err) => VrcSnapshot {
            loaded: false,
            plan: VrcPlanFile::default(),
            error: Some(format!("{}: {}", path.display(), err)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn missing_file_is_empty_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(!snap.loaded);
        assert!(snap.error.is_none());
    }

    #[test]
    fn parses_minimal_shape() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vrc-plan.json"),
            r#"{"mode":"full","reason":"no changed files detected"}"#,
        )
        .unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(snap.loaded);
        assert_eq!(snap.plan.mode, "full");
        assert_eq!(snap.plan.reason, "no changed files detected");
        assert!(snap.plan.selected_tests.is_empty());
    }

    #[test]
    fn parses_richer_shape() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vrc-plan.json"),
            r#"{
                "mode":"selected",
                "reason":"VTI selection from 3 changed files",
                "selected_tests":["unit::a","unit::b"],
                "skipped_tests":["integration::c"],
                "confidence":0.92
            }"#,
        )
        .unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(snap.loaded);
        assert_eq!(snap.plan.selected_tests.len(), 2);
        assert_eq!(snap.plan.skipped_tests.len(), 1);
        assert_eq!(snap.plan.confidence, Some(0.92));
    }
}
