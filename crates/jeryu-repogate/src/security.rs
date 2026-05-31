//! Source security scan ported from `scripts/security-scan.py`.

use std::path::{Path, PathBuf};

use crate::outcome::{GateOutcome, relative_display};

/// Build the source-pattern needles the security scan refuses to find in Rust
/// sources.
///
/// The needles are assembled at runtime from fragments so the forbidden
/// patterns never appear contiguously in this source file — otherwise the
/// scanner would flag its own definition when run over the workspace. The
/// decoded needles are byte-for-byte identical to the legacy
/// `security-scan.py` `blocked` list.
pub fn security_blocked() -> Vec<String> {
    vec![
        // Assembles the unsafe-block opener ("unsafe" + " " + "{").
        format!("{}{}", "unsafe", " {"),
        // Assembles the shell-spawning Command constructor call.
        format!("{}{}{}{}", "std::process::Command::new(", "\"", "sh", "\")"),
    ]
}

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
    let needles = security_blocked();
    let mut violations: Vec<(String, String)> = Vec::new();

    for scan_root in SECURITY_SCAN_ROOTS {
        let dir = root.join(scan_root);
        let mut sources = Vec::new();
        collect_rust_sources(&dir, false, &mut sources)?;
        for path in sources {
            let text = std::fs::read_to_string(&path)?;
            let display = relative_display(root, &path);
            for needle in &needles {
                if text.contains(needle.as_str()) {
                    violations.push((display.clone(), needle.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
        // The forbidden pattern is assembled at runtime so this test source
        // never embeds the literal needle verbatim (which would self-flag the scan).
        let unsafe_needle = format!("{}{}", "unsafe", " {");
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("bins/bar/src")).unwrap();
        fs::write(
            dir.path().join("bins/bar/src/main.rs"),
            format!("fn main() {{ {unsafe_needle} }} }}\n"),
        )
        .unwrap();
        let outcome = run_security_scan(dir.path()).unwrap();
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stdout.len(), 1);
        assert!(outcome.stdout[0].starts_with("security violation "));
        assert!(outcome.stdout[0].ends_with(&format!(": {unsafe_needle}")));
        assert!(outcome.stdout[0].contains("bins/bar/src/main.rs"));
    }

    #[test]
    fn security_scan_flags_shell_command_and_skips_dot_underscore() {
        // Both forbidden patterns are assembled at runtime so this test source
        // never embeds the literal needles (which would self-flag the scan).
        let needles = security_blocked();
        let unsafe_needle = &needles[0];
        let shell_needle = &needles[1];
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("crates/baz/src")).unwrap();
        fs::write(
            dir.path().join("crates/baz/src/lib.rs"),
            format!("let c = {shell_needle};\n"),
        )
        .unwrap();
        // AppleDouble file containing a violation must be ignored.
        fs::write(
            dir.path().join("crates/baz/src/._lib.rs"),
            format!("{unsafe_needle} }}\n"),
        )
        .unwrap();
        // File under a ._-prefixed directory must be ignored.
        fs::create_dir_all(dir.path().join("crates/baz/._hidden")).unwrap();
        fs::write(
            dir.path().join("crates/baz/._hidden/x.rs"),
            format!("{unsafe_needle} }}\n"),
        )
        .unwrap();
        let outcome = run_security_scan(dir.path()).unwrap();
        assert_eq!(outcome.exit_code, 1);
        assert_eq!(outcome.stdout.len(), 1);
        assert!(
            outcome.stdout[0].ends_with(&format!(": {shell_needle}")),
            "got: {}",
            outcome.stdout[0]
        );
    }
}
