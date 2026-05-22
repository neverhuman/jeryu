use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use regex::Regex;
use walkdir::WalkDir;

use super::*;

pub(crate) fn scan_function_lengths(
    root: &Path,
    manifest_path: &Path,
    package: &cargo_vrc::workspace::PackageSnapshot,
    existing: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let contents = read_file_contents(manifest_path)?;
    let func_re =
        Regex::new(r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)")?;
    let mut longest = 0usize;
    let mut longest_fn = String::new();
    for cap in func_re.captures_iter(&contents) {
        let Some(name_match) = cap.get(1) else {
            continue;
        };
        let name = name_match.as_str();
        let len = contents
            .lines()
            .skip_while(|line| !line.contains(name))
            .take_while(|line| !line.trim_start().starts_with("fn "))
            .count();
        if len > longest {
            longest = len;
            longest_fn = name.to_string();
        }
    }
    if longest > 150 {
        findings.push(Finding {
            class_id: "function-too-long".to_string(),
            severity: "warning".to_string(),
            confidence: 0.72,
            path: display_relative(root, manifest_path),
            summary: format!(
                "{} has a long function `{}` at {} lines",
                package.name, longest_fn, longest
            ),
            suggested_fix:
                "Split the function into smaller named helpers that each own one responsibility."
                    .to_string(),
            existing_exception: existing("function-too-long"),
        });
    }
    Ok(findings)
}

pub(crate) fn scan_unsafe_blocks(
    root: &Path,
    manifest_path: &Path,
    package: &cargo_vrc::workspace::PackageSnapshot,
    existing: impl Fn(&str) -> Option<String>,
) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    let contents = read_file_contents(manifest_path)?;
    // SAFETY: this scan only counts textual `unsafe` markers in source text;
    // it does not evaluate or execute any unsafe Rust code.
    let unsafe_marker_count = contents.matches("unsafe").count();
    if unsafe_marker_count > 8 {
        findings.push(Finding {
            class_id: "unsafe-surface-heavy".to_string(),
            severity: "warning".to_string(),
            confidence: 0.8,
            path: display_relative(root, manifest_path),
            // SAFETY: lexical scan only; this counts textual `unsafe` markers.
            summary: format!(
                "{} hits the unsafe token {} times",
                package.name, unsafe_marker_count
            ),
            suggested_fix: "Audit unsafe blocks for narrower wrappers or safe abstractions."
                .to_string(),
            existing_exception: existing("unsafe-surface-heavy"),
        });
    }
    Ok(findings)
}

pub(crate) fn package_source_file_count(package_root: &Path) -> usize {
    WalkDir::new(package_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .count()
}

pub(crate) fn package_has_hidden_io(package_root: &Path) -> bool {
    WalkDir::new(package_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .any(|content| hidden_io_signal(&content))
}

pub(crate) fn read_file_contents(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}
