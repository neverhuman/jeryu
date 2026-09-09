//! 500-job CI fixture generation ported from `scripts/generate-500-job-fixture.py`.

use std::path::Path;

use crate::outcome::GateOutcome;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
