//! Read and rewrite the single `[workspace.package] version` line, and gate that
//! every workspace member inherits it.
//!
//! The single source of truth is the one `version = "X.Y.Z"` line under
//! `[workspace.package]` in the root `Cargo.toml`. The rewrite is line-precise
//! (not a full TOML round-trip) so comments, formatting, and the
//! `[workspace.dependencies]` block are preserved byte-for-byte. Member crates
//! are never touched: new crates inherit automatically by declaring
//! `version.workspace = true`, and [`assert_members_inherit`] turns any pinned
//! straggler into a fixable lint.

use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::semver::Version;

/// Read the workspace version from `[workspace.package].version`.
///
/// # Errors
/// Returns an error if the root manifest cannot be read/parsed or the version
/// key is missing or malformed.
pub fn read_workspace_version(root: &Path) -> Result<Version> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .with_context(|| format!("read {}", root.join("Cargo.toml").display()))?;
    let doc: toml::Value = toml::from_str(&text).context("parse root Cargo.toml")?;
    let s = doc
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .context("[workspace.package].version missing")?;
    Version::parse(s)
}

/// Rewrite ONLY the `[workspace.package].version` line to `v`, leaving every
/// other byte of the manifest (comments, member list, dependency table)
/// untouched.
///
/// # Errors
/// Returns an error if the manifest cannot be read/written or the
/// `[workspace.package]` version line cannot be located.
pub fn write_workspace_version(root: &Path, v: Version) -> Result<()> {
    let path = root.join("Cargo.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut in_ws_pkg = false;
    let mut done = false;
    let mut out = String::with_capacity(text.len() + 8);
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_ws_pkg = trimmed.starts_with("[workspace.package]");
        }
        if in_ws_pkg && !done && trimmed.starts_with("version") && trimmed.contains('=') {
            out.push_str(&format!("version = \"{v}\"\n"));
            done = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    ensure!(
        done,
        "did not find [workspace.package] version line to rewrite"
    );
    std::fs::write(&path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Return the list of workspace members that do NOT inherit the workspace
/// version (i.e. they pin a literal `version = "..."` instead of
/// `version.workspace = true`). An empty result means every member is governed
/// by the single source of truth.
///
/// # Errors
/// Returns an error if the root manifest or any member manifest cannot be
/// read/parsed.
pub fn assert_members_inherit(root: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).context("read root Cargo.toml")?;
    let doc: toml::Value = toml::from_str(&text).context("parse root Cargo.toml")?;
    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(toml::Value::as_array)
        .context("[workspace].members missing")?;

    let mut offenders = Vec::new();
    for member in members {
        let Some(rel) = member.as_str() else { continue };
        let manifest_path = root.join(rel).join("Cargo.toml");
        let src = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("read member manifest {}", manifest_path.display()))?;
        let value: toml::Value = toml::from_str(&src)
            .with_context(|| format!("parse member manifest {}", manifest_path.display()))?;
        let inherits = value
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|version| version.get("workspace"))
            .and_then(toml::Value::as_bool)
            == Some(true);
        if !inherits {
            offenders.push(rel.to_string());
        }
    }
    Ok(offenders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    const ROOT_MANIFEST: &str = "\
# top comment preserved
[workspace]
members = [
  \"crates/a\",
  \"crates/b\",
]

[workspace.package]
version = \"4.0.0\"
edition = \"2024\"

[workspace.dependencies]
anyhow = \"1\"
";

    fn write_member(root: &Path, name: &str, version_line: &str) {
        let dir = root.join("crates").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\n{version_line}\nedition.workspace = true\n"),
        )
        .unwrap();
    }

    #[test]
    fn read_returns_workspace_version() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), ROOT_MANIFEST).unwrap();
        assert_eq!(
            read_workspace_version(dir.path()).unwrap().to_string(),
            "4.0.0"
        );
    }

    #[test]
    fn write_rewrites_only_the_one_line() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), ROOT_MANIFEST).unwrap();
        write_workspace_version(dir.path(), Version::parse("4.1.0").unwrap()).unwrap();
        let after = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        // The workspace.package version changed...
        assert!(after.contains("version = \"4.1.0\""));
        assert!(!after.contains("version = \"4.0.0\""));
        // ...and everything else is byte-for-byte identical (only that one line differs).
        let expected = ROOT_MANIFEST.replace("version = \"4.0.0\"", "version = \"4.1.0\"");
        assert_eq!(after, expected);
        // Round-trip parse confirms.
        assert_eq!(
            read_workspace_version(dir.path()).unwrap().to_string(),
            "4.1.0"
        );
    }

    #[test]
    fn write_does_not_touch_dependency_table_or_comments() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), ROOT_MANIFEST).unwrap();
        write_workspace_version(dir.path(), Version::parse("5.0.0").unwrap()).unwrap();
        let after = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(after.contains("# top comment preserved"));
        assert!(after.contains("anyhow = \"1\""));
        assert!(after.contains("members = ["));
    }

    #[test]
    fn write_errors_when_no_version_line() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace.package]\nedition = \"2024\"\n",
        )
        .unwrap();
        assert!(write_workspace_version(dir.path(), Version::parse("4.0.1").unwrap()).is_err());
    }

    #[test]
    fn members_inherit_reports_only_stragglers() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), ROOT_MANIFEST).unwrap();
        write_member(dir.path(), "a", "version.workspace = true");
        write_member(dir.path(), "b", "version = \"0.1.0\"");
        let offenders = assert_members_inherit(dir.path()).unwrap();
        assert_eq!(offenders, vec!["crates/b".to_string()]);
    }

    #[test]
    fn members_inherit_empty_when_all_inherit() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), ROOT_MANIFEST).unwrap();
        write_member(dir.path(), "a", "version.workspace = true");
        write_member(dir.path(), "b", "version.workspace = true");
        assert!(assert_members_inherit(dir.path()).unwrap().is_empty());
    }
}
