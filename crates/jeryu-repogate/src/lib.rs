//! Governance/CI gate logic ported from the legacy `scripts/*.py` repo gates.
//!
//! Each gate is exposed as a pure function operating on an explicit repository
//! root so the behavior can be exercised by behavioral tests without depending
//! on the process working directory. The binary wrapper applies the resulting
//! exit codes and stdout signal lines, preserving the original CLI contracts.
//!
//! The gates live in dedicated submodules grouped by command/check, but every
//! public item is re-exported here so the original `jeryu_repogate::Thing`
//! import paths keep resolving unchanged.

mod fixture;
mod outcome;
mod release;
mod score;
mod security;

pub use fixture::{FIXTURE_JOB_COUNT, FIXTURE_RELATIVE_PATH, build_fixture, run_gen_fixture};
pub use outcome::GateOutcome;
pub use release::{RELEASE_REQUIRED_PATHS, run_release_gate};
pub use score::{
    REQUIRED_EXIT_SCORE, RepoScore, SCORE_REQUIRED_MEMBERS, SCORE_REQUIRED_PATHS,
    compute_repo_score, repo_score_json, run_score,
};
pub use security::{SECURITY_SCAN_ROOTS, run_security_scan, security_blocked};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Materialize every path the score + release gates expect to find.
    ///
    /// Shared by the score and release gate tests in their respective modules.
    pub(crate) fn write_full_repo(root: &Path) {
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
}
