//! Port of `scripts/check-fixtures.py`: every `tests/fixtures/**/*.json` parses.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::report::GateReport;

/// Port of `scripts/check-fixtures.py`.
///
/// Recursively parses every `*.json` file under `fixtures_dir`, skipping
/// AppleDouble (`._*`) sidecar files and any path component beginning with
/// `._`. A parse failure fails the gate.
///
/// # Errors
/// Returns an error if the fixtures directory cannot be traversed, if a file
/// cannot be read, or if a fixture is not valid JSON (mirroring the Python
/// `json.loads` raising).
pub fn fixtures(fixtures_dir: &Path) -> Result<GateReport> {
    let mut json_files = Vec::new();
    collect_json_files(fixtures_dir, &mut json_files)?;
    json_files.sort();

    for path in json_files {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str::<serde_json::Value>(&text)
            .with_context(|| format!("parsing {} as JSON", path.display()))?;
    }

    Ok(GateReport::pass("fixtures ok"))
}

/// Whether a single path component is an AppleDouble sidecar.
fn is_apple_double(component: &str) -> bool {
    component.starts_with("._")
}

/// Recursively gather `*.json` files, skipping any `._`-prefixed component.
fn collect_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("reading entry in {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("stat {}", path.display()))?;
        if file_type.is_dir() {
            collect_json_files(&path, out)?;
        } else if file_type.is_file() {
            // Mirror Python: skip if the file (or any path part) starts with "._",
            // and only consider files ending in ".json".
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".json") {
                continue;
            }
            // Check every path component for a "._" prefix, matching
            // `any(part.startswith("._") for part in path.parts)` plus the
            // explicit `path.name.startswith("._")` guard in the Python.
            let skip = path
                .components()
                .any(|c| is_apple_double(&c.as_os_str().to_string_lossy()));
            if skip || is_apple_double(&name) {
                continue;
            }
            out.push(path);
        }
    }
    Ok(())
}
