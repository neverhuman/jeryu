//! Owner: Release subsystem — load in-flight releases from `ops/releases/`
//! Proof: `cargo nextest run -p jeryu -- release::draft_loader`
//! Invariants: read-only; missing files yield empty snapshot; never panics.

use std::path::Path;

use serde::Deserialize;

use crate::tui::app::{ReleaseStageCard, ReleaseStageSnapshot};

/// On-disk shape of `ops/releases/<version>/release-attempt.json`.
/// Only the fields the TUI surface actually displays are pulled out;
/// everything else is ignored via `#[serde(default)]`.
#[derive(Debug, Clone, Default, Deserialize)]
struct ReleaseAttemptFile {
    #[serde(default)]
    attempt_id: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    sha: String,
    #[serde(default)]
    started_at: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    release_manager: String,
}

/// Walk `<repo_root>/ops/releases/*/release-attempt.json` and build a
/// `ReleaseStageSnapshot`. Missing directory → empty. Parse errors on
/// individual files are skipped (with the file logged via `tracing`).
pub fn load_release_stage_snapshot(repo_root: &Path) -> ReleaseStageSnapshot {
    let releases_dir = repo_root.join("ops").join("releases");
    let Ok(entries) = std::fs::read_dir(&releases_dir) else {
        return ReleaseStageSnapshot::default();
    };

    let mut snap = ReleaseStageSnapshot::default();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let attempt_path = path.join("release-attempt.json");
        let Ok(raw) = std::fs::read_to_string(&attempt_path) else {
            continue;
        };
        let attempt: ReleaseAttemptFile = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(target: "tui::release", path = %attempt_path.display(), %err, "skipping malformed release-attempt.json");
                continue;
            }
        };
        let card = ReleaseStageCard {
            label: if attempt.version.is_empty() {
                path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| attempt.attempt_id.clone())
            } else {
                attempt.version.clone()
            },
            agent_id: if attempt.release_manager.is_empty() {
                attempt.sha.get(..8).map(str::to_string).unwrap_or_default()
            } else {
                attempt.release_manager.clone()
            },
            age: friendly_age(&attempt.started_at),
        };
        match stage_for_status(&attempt.status) {
            ReleaseStage::Plan => snap.plan.push(card),
            ReleaseStage::Build => snap.build.push(card),
            ReleaseStage::Proof => snap.proof.push(card),
            ReleaseStage::Canary => snap.canary.push(card),
            ReleaseStage::Stable => snap.stable.push(card),
            ReleaseStage::Skip => {} // status that doesn't map to any pipeline column
        }
    }
    snap
}

#[derive(Debug, Clone, Copy)]
enum ReleaseStage {
    Plan,
    Build,
    Proof,
    Canary,
    Stable,
    Skip,
}

fn stage_for_status(status: &str) -> ReleaseStage {
    match status {
        "draft" | "planning" | "plan" => ReleaseStage::Plan,
        "building" | "build" => ReleaseStage::Build,
        "proving" | "proof" | "validating" | "validation" => ReleaseStage::Proof,
        "canary" | "canarying" | "in-flight" | "canary-authorized" => ReleaseStage::Canary,
        "released" | "stable" | "green" => ReleaseStage::Stable,
        // Example/sample entries shipped in the repo for testing: surface
        // under Plan so demos look populated but reality stays distinct
        // (`agent_id` shows the manager so operators can spot example data).
        "example" => ReleaseStage::Plan,
        _ => ReleaseStage::Skip,
    }
}

fn friendly_age(started_at: &str) -> String {
    if started_at.is_empty() {
        return "unknown".into();
    }
    let Ok(when) = chrono::DateTime::parse_from_rfc3339(started_at) else {
        return started_at.to_string();
    };
    let now = chrono::Utc::now();
    let dur = now.signed_duration_since(when.with_timezone(&chrono::Utc));
    let secs = dur.num_seconds();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn empty_when_releases_dir_missing() {
        let repo = tempfile::tempdir().unwrap();
        let snap = load_release_stage_snapshot(repo.path());
        assert_eq!(snap.total(), 0);
    }

    #[test]
    fn buckets_attempts_into_stages_by_status() {
        let repo = tempfile::tempdir().unwrap();
        let releases = repo.path().join("ops").join("releases");
        fs::create_dir_all(releases.join("v1.0.0")).unwrap();
        fs::create_dir_all(releases.join("v1.1.0")).unwrap();
        fs::write(
            releases.join("v1.0.0").join("release-attempt.json"),
            r#"{"attempt_id":"a","version":"v1.0.0","sha":"abcdef","started_at":"2024-01-01T00:00:00Z","status":"released","release_manager":"alice"}"#,
        )
        .unwrap();
        fs::write(
            releases.join("v1.1.0").join("release-attempt.json"),
            r#"{"attempt_id":"b","version":"v1.1.0","sha":"123456","started_at":"2024-01-02T00:00:00Z","status":"canary","release_manager":"bob"}"#,
        )
        .unwrap();
        let snap = load_release_stage_snapshot(repo.path());
        assert_eq!(snap.stable.len(), 1);
        assert_eq!(snap.canary.len(), 1);
        assert_eq!(snap.stable[0].label, "v1.0.0");
        assert_eq!(snap.canary[0].label, "v1.1.0");
    }

    #[test]
    fn skips_malformed_files() {
        let repo = tempfile::tempdir().unwrap();
        let releases = repo.path().join("ops").join("releases");
        fs::create_dir_all(releases.join("bad")).unwrap();
        fs::write(
            releases.join("bad").join("release-attempt.json"),
            "not json",
        )
        .unwrap();
        let snap = load_release_stage_snapshot(repo.path());
        assert_eq!(snap.total(), 0);
    }
}
