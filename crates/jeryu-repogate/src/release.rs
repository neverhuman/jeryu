//! Release readiness gate ported from `scripts/release-gate.py`.

use std::path::Path;

use crate::outcome::GateOutcome;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::write_full_repo;
    use std::fs;

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
}
