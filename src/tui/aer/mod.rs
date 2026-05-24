//! Owner: Interactive TUI subsystem — AER (Audit Error Report) loader
//! Proof: `cargo nextest run -p jeryu -- tui::aer`
//! Invariants: read-only; non-fatal; surfaces findings emitted by `cargo aer scan`.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AerReport {
    #[serde(default)]
    pub generated_at: String,
    #[serde(default)]
    pub workspace_root: String,
    #[serde(default)]
    pub findings: Vec<AerFinding>,
    #[serde(default)]
    pub repair_hint: AerRepairHint,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AerFinding {
    #[serde(default)]
    pub class_id: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub suggested_fix: String,
    #[serde(default)]
    pub existing_exception: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AerRepairHint {
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub common_fixes: Vec<String>,
    #[serde(default)]
    pub docs_url: String,
    /// The CLI command to rerun. Sometimes called `repair_hint` in the file.
    #[serde(default)]
    pub repair_hint: String,
}

/// Snapshot held on `TuiStateSnapshot`. `loaded == false` means the file
/// was not present (which is normal — only set true once a parse succeeds).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AerSnapshot {
    pub loaded: bool,
    pub report: AerReport,
    pub error: Option<String>,
}

/// Load `<repo_root>/aer-findings.json`. Missing file → empty snapshot.
/// Parse failure → snapshot with `error` set; previous snapshot should be
/// preserved by the caller in that case.
pub fn load_snapshot() -> AerSnapshot {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    load_snapshot_from(&repo_root)
}

pub(crate) fn load_snapshot_from(repo_root: &Path) -> AerSnapshot {
    let path = repo_root.join("aer-findings.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return AerSnapshot::default(),
        Err(err) => {
            return AerSnapshot {
                loaded: false,
                report: AerReport::default(),
                error: Some(format!("{}: {}", path.display(), err)),
            };
        }
    };
    match serde_json::from_str::<AerReport>(&raw) {
        Ok(report) => AerSnapshot {
            loaded: true,
            report,
            error: None,
        },
        Err(err) => AerSnapshot {
            loaded: false,
            report: AerReport::default(),
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
        assert!(snap.report.findings.is_empty());
    }

    #[test]
    fn parses_real_shape() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("aer-findings.json"),
            r#"{
                "generated_at": "2026-05-24",
                "workspace_root": ".",
                "findings": [
                    {
                        "class_id": "x", "severity": "warning", "confidence": 0.5,
                        "path": "a/b", "summary": "s", "suggested_fix": "f"
                    }
                ],
                "repair_hint": {
                    "purpose": "p", "reason": "r", "common_fixes": ["c"],
                    "docs_url": "d", "repair_hint": "cmd"
                }
            }"#,
        )
        .unwrap();
        let snap = load_snapshot_from(dir.path());
        assert!(snap.loaded);
        assert_eq!(snap.report.findings.len(), 1);
        assert_eq!(snap.report.findings[0].severity, "warning");
        assert_eq!(snap.report.repair_hint.repair_hint, "cmd");
    }
}
