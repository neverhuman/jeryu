//! Repo score gate ported from `scripts/score-repo.py`.

use std::path::Path;

use serde::Serialize;

use crate::outcome::GateOutcome;

/// Structured result of the repo score gate, mirroring `score-repo.py`.
///
/// Field order is significant: it determines the key order of the emitted JSON
/// so the document is byte-identical to the Python `json.dumps(..., indent=2)`
/// output that baseline comparisons depend on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepoScore {
    /// Repository identifier.
    pub repo: String,
    /// Phase number.
    pub phase: u32,
    /// Computed score, clamped at 0.
    pub score: i32,
    /// Score required for the gate to pass.
    pub required_exit_score: i32,
    /// Advisories promoted to hard blocks when the score is below threshold.
    pub hard_blocks: Vec<String>,
    /// All advisories raised during scoring.
    pub advisories: Vec<String>,
}

/// Required top-level repository artifacts checked by the score gate.
pub const SCORE_REQUIRED_PATHS: &[&str] = &[
    "AGENTS.md",
    "Justfile",
    "rust-toolchain.toml",
    "Cargo.toml",
    "agent/owner-map.json",
    "agent/test-map.json",
    "agent/ci-lanes.toml",
    "agent/proof-lanes.toml",
    "agent/generated-zones.toml",
    "agent/baselines/main.repo-score.json",
    "docs/engineering_spec.md",
    "docs/PHASE12_SPEC.md",
    "policies/cache-laws.toml",
];

/// Workspace members the score gate expects to find referenced in `Cargo.toml`.
pub const SCORE_REQUIRED_MEMBERS: &[&str] = &[
    "crates/jeryu-cache-core",
    "crates/jeryu-cache-service",
    "crates/jeryu-runner-core",
    "crates/jeryu-rustjet",
    "crates/jeryu-gitd",
];

/// Score required for the repo score gate to exit successfully.
pub const REQUIRED_EXIT_SCORE: i32 = 95;

/// Compute the repository score exactly as `score-repo.py` does.
///
/// Each missing required artifact subtracts 8 points; each missing workspace
/// member reference subtracts 3 points. The reported score is clamped at 0.
/// `hard_blocks` is empty unless the (pre-clamp) score dropped below 95, in
/// which case it equals the full advisory list, matching the Python logic.
pub fn compute_repo_score(root: &Path) -> std::io::Result<RepoScore> {
    let mut score: i32 = 100;
    let mut advisories: Vec<String> = Vec::new();

    for raw in SCORE_REQUIRED_PATHS {
        if !root.join(raw).exists() {
            score -= 8;
            advisories.push(format!("missing {raw}"));
        }
    }

    let cargo_path = root.join("Cargo.toml");
    let workspace = if cargo_path.exists() {
        std::fs::read_to_string(&cargo_path)?
    } else {
        String::new()
    };

    for member in SCORE_REQUIRED_MEMBERS {
        if !workspace.contains(member) {
            score -= 3;
            advisories.push(format!("workspace missing {member}"));
        }
    }

    let hard_blocks = if score >= REQUIRED_EXIT_SCORE {
        Vec::new()
    } else {
        advisories.clone()
    };

    Ok(RepoScore {
        repo: "jeryu".to_string(),
        phase: 12,
        score: score.max(0),
        required_exit_score: REQUIRED_EXIT_SCORE,
        hard_blocks,
        advisories,
    })
}

/// Serialize a [`RepoScore`] to match Python's `json.dumps(result, indent=2)`.
///
/// `serde_json::to_string_pretty` uses the same 2-space indentation and one
/// element per array line, and the struct field order fixes the key order, so
/// the output is byte-identical to the retired script.
pub fn repo_score_json(score: &RepoScore) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(score)
}

/// Run the repo score gate, optionally rewriting the baseline file.
///
/// Mirrors `score-repo.py`: the JSON document is always printed; with
/// `write_baseline` set it is first written (plus a trailing newline) to
/// `agent/baselines/main.repo-score.json`. Exit code is 1 when the score is
/// below the required exit score, otherwise 0.
pub fn run_score(root: &Path, write_baseline: bool) -> std::io::Result<GateOutcome> {
    let result = compute_repo_score(root)?;
    let json = repo_score_json(&result)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    if write_baseline {
        let baseline = root.join("agent/baselines/main.repo-score.json");
        if let Some(parent) = baseline.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&baseline, format!("{json}\n"))?;
    }

    let exit_code = i32::from(result.score < result.required_exit_score);
    Ok(GateOutcome {
        stdout: vec![json],
        exit_code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::write_full_repo;
    use std::fs;

    #[test]
    fn score_passes_on_complete_repo() {
        let dir = tempfile::tempdir().unwrap();
        write_full_repo(dir.path());
        let outcome = run_score(dir.path(), false).unwrap();
        assert_eq!(outcome.exit_code, 0);
        let score = compute_repo_score(dir.path()).unwrap();
        assert_eq!(score.score, 100);
        assert!(score.advisories.is_empty());
        assert!(score.hard_blocks.is_empty());
    }

    #[test]
    fn score_fails_and_promotes_hard_blocks_when_artifacts_missing() {
        let dir = tempfile::tempdir().unwrap();
        // Empty repo: every required path missing, plus all workspace members.
        // plus members missing (Cargo.toml absent) => 5*3 = 15. Score clamps to 0.
        let score = compute_repo_score(dir.path()).unwrap();
        assert_eq!(score.score, 0);
        assert_eq!(
            score.advisories.len(),
            SCORE_REQUIRED_PATHS.len() + SCORE_REQUIRED_MEMBERS.len()
        );
        // Below threshold => hard_blocks mirrors advisories.
        assert_eq!(score.hard_blocks, score.advisories);
        assert!(score.advisories.contains(&"missing AGENTS.md".to_string()));
        assert!(
            score
                .advisories
                .contains(&"workspace missing crates/jeryu-gitd".to_string())
        );

        let outcome = run_score(dir.path(), false).unwrap();
        assert_eq!(outcome.exit_code, 1);
    }

    #[test]
    fn score_single_missing_artifact_stays_above_threshold() {
        let dir = tempfile::tempdir().unwrap();
        write_full_repo(dir.path());
        // Remove one file: 100 - 8 = 92 < 95 => fail, hard_blocks populated.
        fs::remove_file(dir.path().join("Justfile")).unwrap();
        let score = compute_repo_score(dir.path()).unwrap();
        assert_eq!(score.score, 92);
        assert_eq!(score.advisories, vec!["missing Justfile".to_string()]);
        assert_eq!(score.hard_blocks, score.advisories);
        assert_eq!(run_score(dir.path(), false).unwrap().exit_code, 1);
    }

    #[test]
    fn score_json_matches_python_indent_and_key_order() {
        let score = RepoScore {
            repo: "jeryu".to_string(),
            phase: 12,
            score: 100,
            required_exit_score: 95,
            hard_blocks: Vec::new(),
            advisories: Vec::new(),
        };
        let json = repo_score_json(&score).unwrap();
        let expected = "{\n  \"repo\": \"jeryu\",\n  \"phase\": 12,\n  \"score\": 100,\n  \"required_exit_score\": 95,\n  \"hard_blocks\": [],\n  \"advisories\": []\n}";
        assert_eq!(json, expected);
    }

    #[test]
    fn score_write_baseline_emits_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        write_full_repo(dir.path());
        run_score(dir.path(), true).unwrap();
        let baseline =
            fs::read_to_string(dir.path().join("agent/baselines/main.repo-score.json")).unwrap();
        assert!(baseline.ends_with("}\n"));
        // Parse round-trips to the same logical document.
        let parsed: serde_json::Value = serde_json::from_str(&baseline).unwrap();
        assert_eq!(parsed["score"], 100);
        assert_eq!(parsed["repo"], "jeryu");
    }
}
