//! Governance/CI gate logic ported from the legacy `scripts/*.py` repo gates.
//!
//! Each gate is exposed as a pure function operating on an explicit repository
//! root so the behavior can be exercised by behavioral tests without depending
//! on the process working directory. The binary wrapper applies the resulting
//! exit codes and stdout signal lines, preserving the original CLI contracts.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Outcome of a gate: the lines to print on stdout and the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateOutcome {
    /// Lines to emit on stdout, in order.
    pub stdout: Vec<String>,
    /// Process exit code (0 = pass).
    pub exit_code: i32,
}

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
/// the output is byte-identical to the legacy script.
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

/// Source-pattern needles the security scan refuses to find in Rust sources.
pub const SECURITY_BLOCKED: &[&str] = &["unsafe {", "std::process::Command::new(\"sh\")"];

/// Roots under which the security scan walks for `*.rs` files.
pub const SECURITY_SCAN_ROOTS: &[&str] = &["crates", "bins"];

/// Recursively collect `*.rs` files under `dir`, skipping AppleDouble (`._`)
/// files and any path containing a `._`-prefixed component, matching the
/// filtering in `security-scan.py`.
fn collect_rust_sources(
    dir: &Path,
    skip_component: bool,
    out: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();

    for path in entries {
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };
        let this_is_dot_underscore = name.starts_with("._");
        if path.is_dir() {
            collect_rust_sources(&path, skip_component || this_is_dot_underscore, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if this_is_dot_underscore || skip_component {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}

/// Run the security scan exactly as `security-scan.py` does.
///
/// Walks `crates/` and `bins/` for `*.rs` files (skipping `._`-prefixed names
/// and any path under a `._`-prefixed directory), reports every blocked-pattern
/// occurrence as `security violation {path}: {needle}` and exits 1, or prints
/// `security scan ok` and exits 0 when clean.
pub fn run_security_scan(root: &Path) -> std::io::Result<GateOutcome> {
    let mut violations: Vec<(String, &str)> = Vec::new();

    for scan_root in SECURITY_SCAN_ROOTS {
        let dir = root.join(scan_root);
        let mut sources = Vec::new();
        collect_rust_sources(&dir, false, &mut sources)?;
        for path in sources {
            let text = std::fs::read_to_string(&path)?;
            let display = relative_display(root, &path);
            for needle in SECURITY_BLOCKED {
                if text.contains(needle) {
                    violations.push((display.clone(), needle));
                }
            }
        }
    }

    if violations.is_empty() {
        Ok(GateOutcome {
            stdout: vec!["security scan ok".to_string()],
            exit_code: 0,
        })
    } else {
        let stdout = violations
            .into_iter()
            .map(|(path, needle)| format!("security violation {path}: {needle}"))
            .collect();
        Ok(GateOutcome {
            stdout,
            exit_code: 1,
        })
    }
}

/// Files the release gate requires to exist, from `release-gate.py`.
pub const RELEASE_REQUIRED_PATHS: &[&str] = &[
    "Cargo.toml",
    "docs/engineering_spec.md",
    "docs/PHASE12_SPEC.md",
    "crates/jeryu-cache-core/src/lib.rs",
    "crates/jeryu-cache-service/src/lib.rs",
    "crates/jeryu-runner-core/src/lib.rs",
    "crates/jeryu-rustjet/src/lib.rs",
    "bins/jeryu-ci-bin/src/main.rs",
];

/// Run the release gate exactly as `release-gate.py` does.
///
/// Reports each missing required file as `release gate missing {path}` and
/// exits 1, or prints `release gate ok` and exits 0 when all are present.
pub fn run_release_gate(root: &Path) -> GateOutcome {
    let missing: Vec<&&str> = RELEASE_REQUIRED_PATHS
        .iter()
        .filter(|path| !root.join(path).exists())
        .collect();

    if missing.is_empty() {
        GateOutcome {
            stdout: vec!["release gate ok".to_string()],
            exit_code: 0,
        }
    } else {
        let stdout = missing
            .into_iter()
            .map(|path| format!("release gate missing {path}"))
            .collect();
        GateOutcome {
            stdout,
            exit_code: 1,
        }
    }
}

/// Default relative output path for the generated 500-job fixture.
pub const FIXTURE_RELATIVE_PATH: &str = "tests/fixtures/github/500_jobs.yml";

/// Number of jobs in the generated CI fixture.
pub const FIXTURE_JOB_COUNT: usize = 500;

/// Build the exact text of the 500-job GitHub Actions fixture.
///
/// Reproduces `generate-500-job-fixture.py`: a `bench-500` workflow with 500
/// jobs `job_000..job_499`, each pinned to the `native-rust-clean` runner, each
/// (except the first) depending on its predecessor, with a single echo step.
/// A trailing newline is appended, matching the Python `"\n".join(...) + "\n"`.
pub fn build_fixture() -> String {
    let mut lines: Vec<String> = vec![
        "name: bench-500".to_string(),
        "on: [push]".to_string(),
        "jobs:".to_string(),
    ];
    for i in 0..FIXTURE_JOB_COUNT {
        let name = format!("job_{i:03}");
        lines.push(format!("  {name}:"));
        lines.push("    runs-on: native-rust-clean".to_string());
        if i != 0 {
            lines.push(format!("    needs: [job_{:03}]", i - 1));
        }
        lines.push("    steps:".to_string());
        lines.push(format!("      - name: check {i}"));
        lines.push(format!("        run: echo job {i}"));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Generate the 500-job fixture and write it under `root`.
///
/// Mirrors `generate-500-job-fixture.py`: creates the parent directory, writes
/// the fixture, and reports the written path (printed by the binary). Exit code
/// is always 0 on success.
pub fn run_gen_fixture(root: &Path, out_relative: &str) -> std::io::Result<GateOutcome> {
    let out = root.join(out_relative);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, build_fixture())?;
    Ok(GateOutcome {
        stdout: vec![out.display().to_string()],
        exit_code: 0,
    })
}

/// Render a path relative to `root` for display, falling back to the full path.
fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Materialize every path the score + release gates expect to find.
    fn write_full_repo(root: &Path) {
        for raw in SCORE_REQUIRED_PATHS {
            let path = root.join(raw);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "x").unwrap();
        }
        // Cargo.toml must reference every required workspace member.
        let mut cargo = String::from("[workspace]\nmembers = [\n");
        for member in SCORE_REQUIRED_MEMBERS {
            cargo.push_str(&format!("  \"{member}\",\n"));
        }
        cargo.push_str("]\n");
        fs::write(root.join("Cargo.toml"), cargo).unwrap();

        for raw in RELEASE_REQUIRED_PATHS {
            let path = root.join(raw);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            if !path.exists() {
                fs::write(&path, "x").unwrap();
            }
        }
    }

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
        // Empty repo: every required path missing => 12*8 = 96 lost from files,
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

    #[test]
    fn security_scan_ok_when_no_blocked_patterns() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/foo/src")).unwrap();
        fs::write(
            dir.path().join("crates/foo/src/lib.rs"),
            "pub fn safe() {}\n",
        )
        .unwrap();
        let outcome = run_security_scan(dir.path()).unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.stdout, vec!["security scan ok".to_string()]);
    }

    #[test]
    fn security_scan_flags_unsafe_block() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("bins/bar/src")).unwrap();
        fs::write(
            dir.path().join("bins/bar/src/main.rs"),
            "fn main() { unsafe { } }\n",
        )
        .unwrap();
        let outcome = run_security_scan(dir.path()).unwrap();
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stdout.len(), 1);
        assert!(outcome.stdout[0].starts_with("security violation "));
        assert!(outcome.stdout[0].ends_with(": unsafe {"));
        assert!(outcome.stdout[0].contains("bins/bar/src/main.rs"));
    }

    #[test]
    fn security_scan_flags_shell_command_and_skips_dot_underscore() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/baz/src")).unwrap();
        fs::write(
            dir.path().join("crates/baz/src/lib.rs"),
            "let c = std::process::Command::new(\"sh\");\n",
        )
        .unwrap();
        // AppleDouble file containing a violation must be ignored.
        fs::write(dir.path().join("crates/baz/src/._lib.rs"), "unsafe { }\n").unwrap();
        // File under a ._-prefixed directory must be ignored.
        fs::create_dir_all(dir.path().join("crates/baz/._hidden")).unwrap();
        fs::write(dir.path().join("crates/baz/._hidden/x.rs"), "unsafe { }\n").unwrap();
        let outcome = run_security_scan(dir.path()).unwrap();
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stdout.len(), 1);
        assert!(
            outcome.stdout[0].ends_with(": std::process::Command::new(\"sh\")"),
            "got: {}",
            outcome.stdout[0]
        );
    }

    #[test]
    fn release_gate_ok_when_all_present() {
        let dir = tempfile::tempdir().unwrap();
        write_full_repo(dir.path());
        let outcome = run_release_gate(dir.path());
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(outcome.stdout, vec!["release gate ok".to_string()]);
    }

    #[test]
    fn release_gate_reports_each_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        // Only create Cargo.toml; the rest are missing.
        fs::write(dir.path().join("Cargo.toml"), "x").unwrap();
        let outcome = run_release_gate(dir.path());
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stdout.len(), RELEASE_REQUIRED_PATHS.len() - 1);
        assert!(
            outcome
                .stdout
                .iter()
                .all(|line| line.starts_with("release gate missing "))
        );
        assert!(
            outcome
                .stdout
                .contains(&"release gate missing docs/engineering_spec.md".to_string())
        );
    }

    #[test]
    fn fixture_has_expected_shape() {
        let text = build_fixture();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "name: bench-500");
        assert_eq!(lines[1], "on: [push]");
        assert_eq!(lines[2], "jobs:");
        assert_eq!(lines[3], "  job_000:");
        assert_eq!(lines[4], "    runs-on: native-rust-clean");
        // job_000 has no `needs` line.
        assert_eq!(lines[5], "    steps:");
        assert_eq!(lines[6], "      - name: check 0");
        assert_eq!(lines[7], "        run: echo job 0");
        // 500 job declarations.
        assert_eq!(text.matches("    runs-on: native-rust-clean").count(), 500);
        // 499 dependency edges.
        assert_eq!(text.matches("    needs: [job_").count(), 499);
        assert!(text.contains("  job_499:"));
        assert!(text.contains("    needs: [job_498]"));
        assert!(text.contains("      - name: check 499"));
        // Trailing newline preserved.
        assert!(text.ends_with("        run: echo job 499\n"));
    }

    #[test]
    fn gen_fixture_writes_file_and_reports_path() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = run_gen_fixture(dir.path(), FIXTURE_RELATIVE_PATH).unwrap();
        assert_eq!(outcome.exit_code, 0);
        let written = dir.path().join(FIXTURE_RELATIVE_PATH);
        assert!(written.exists());
        assert_eq!(outcome.stdout, vec![written.display().to_string()]);
        let text = fs::read_to_string(&written).unwrap();
        assert_eq!(text, build_fixture());
        // The compiler test consumes exactly 500 jobs / 499 edges.
        assert_eq!(
            text.lines()
                .filter(|l| l.ends_with(':') && l.starts_with("  job_"))
                .count(),
            500
        );
    }
}
