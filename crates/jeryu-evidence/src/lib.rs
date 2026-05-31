//! Zero-evidence guard: scan a workspace for forbidden third-party forge brand
//! markers.
//!
//! The forbidden literals are stored only as hex-encoded strings and decoded at
//! runtime, so this crate's own source can never match itself (the scanner stays
//! self-clean).

use std::fs;
use std::path::{Path, PathBuf};

/// Hex-encoded forbidden brand markers.
///
/// Storing the markers as hex keeps the literal bytes out of this source file,
/// so the scanner never flags itself. Each entry is the lowercase byte sequence
/// of one forbidden third-party forge brand or product name.
const BLOCKED_MARKER_HEX: &[&str] = &[
    "6769746c6162",
    "676974206c6162",
    "6769742d6c6162",
    "2e6769746c61622d63692e796d6c",
    "676c6162",
    "6a6974666f726765",
    "6e6974726f",
    "63726174657661756c74",
    "6d6972726f727661756c74",
    "62656e63686c6162",
];

/// Directory names whose entire subtree is skipped during a scan.
///
/// `.git`, `.worktrees`, `target`, and `.jankurai` hold VCS internals, build
/// output, and generated audit reports that are never product source.
/// `node_modules`, `dist`, and `storybook-static` cover the web build artifacts
/// and local Playwright HTML reports. External worktree roots live outside the
/// repo, so a default `.` scan never reaches them.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".worktrees",
    ".jankurai",
    "target",
    "node_modules",
    "dist",
    "playwright-report",
    "storybook-static",
];

/// Generated file suffixes skipped even when they appear outside skipped dirs.
const SKIP_FILE_SUFFIXES: &[&str] = &[".tsbuildinfo"];

/// Repo-root narrative/coordination files excluded from the brand scan. These
/// document the migration *away from* the retired provider, so referencing it
/// by name is unavoidable and is not a product- or wire-surface leak. Mirrors
/// the jankurai `agent/audit-policy.toml` `excluded_paths` for the same files.
const SKIP_FILES: &[&str] = &["AGENT_CHAT.md", "CI_TRACKER.md"];

/// Errors that can occur while scanning a workspace.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// An I/O failure while walking or reading a path.
    #[error("io error at {path}: {source}")]
    Io {
        /// The path that triggered the failure.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// One of the embedded hex markers failed to decode. This indicates a
    /// programming error in the marker table, not a problem with the scanned
    /// tree.
    #[error("invalid embedded marker {marker:?}: {source}")]
    InvalidMarker {
        /// The offending hex marker.
        marker: String,
        /// The decode error.
        source: hex::FromHexError,
    },
}

/// A single forbidden-marker finding, rendered as
/// `"{rel}:{line}: blocked marker"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path relative to the scanned root.
    pub rel: PathBuf,
    /// One-based line number where the marker was found.
    pub line: usize,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: blocked marker", self.rel.display(), self.line)
    }
}

/// Decode the embedded hex markers into raw byte patterns.
fn blocked_markers() -> Result<Vec<Vec<u8>>, ScanError> {
    BLOCKED_MARKER_HEX
        .iter()
        .map(|value| {
            hex::decode(value).map_err(|source| ScanError::InvalidMarker {
                marker: (*value).to_string(),
                source,
            })
        })
        .collect()
}

/// Recursively collect scannable files under `root`, skipping [`SKIP_DIRS`].
///
/// Returns `(relative_path, absolute_path)` pairs for regular files only.
fn iter_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>, ScanError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|source| ScanError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| ScanError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| ScanError::Io {
                path: path.clone(),
                source,
            })?;

            let rel = path.strip_prefix(root).map_err(|_| ScanError::Io {
                path: path.clone(),
                source: std::io::Error::other("path escaped scan root"),
            })?;

            // Skip any path whose components include a skipped directory name.
            if rel
                .components()
                .any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
            {
                continue;
            }

            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let rel_text = rel.to_string_lossy();
                if SKIP_FILE_SUFFIXES
                    .iter()
                    .any(|suffix| rel_text.ends_with(suffix))
                {
                    continue;
                }
                if SKIP_FILES.iter().any(|skip| rel_text == *skip) {
                    continue;
                }
                out.push((rel.to_path_buf(), path));
            }
            // Symlinks and other non-regular entries are ignored; only regular
            // files are scanned.
        }
    }
    Ok(out)
}

/// Compute the one-based line number of byte offset `index` in `data`
/// (count of `\n` before the index, plus 1).
fn line_number(data: &[u8], index: usize) -> usize {
    data[..index].iter().filter(|&&b| b == b'\n').count() + 1
}

/// Find the first occurrence of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Scan `root` for forbidden brand markers, returning one [`Finding`] per
/// offending file (the first matching marker wins; scanning of that file stops
/// at the first hit).
///
/// File contents are lowercased before matching so the comparison is
/// case-insensitive.
///
/// # Errors
///
/// Returns [`ScanError`] if a path cannot be read or an embedded marker fails
/// to decode.
pub fn scan(root: &Path) -> Result<Vec<Finding>, ScanError> {
    let markers = blocked_markers()?;
    let mut findings = Vec::new();
    for (rel, path) in iter_files(root)? {
        let raw = fs::read(&path).map_err(|source| ScanError::Io {
            path: path.clone(),
            source,
        })?;
        let data: Vec<u8> = raw.iter().map(u8::to_ascii_lowercase).collect();
        for marker in &markers {
            if let Some(index) = find_subslice(&data, marker) {
                findings.push(Finding {
                    rel,
                    line: line_number(&data, index),
                });
                break;
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_decode_to_expected_count() {
        let markers = blocked_markers().expect("markers decode");
        assert_eq!(markers.len(), BLOCKED_MARKER_HEX.len());
        // First marker decodes to the six-byte forbidden brand name.
        assert_eq!(markers[0].len(), 6);
    }

    #[test]
    fn line_number_counts_newlines() {
        let data = b"aa\nbb\ncc";
        assert_eq!(line_number(data, 0), 1);
        assert_eq!(line_number(data, 3), 2);
        assert_eq!(line_number(data, 6), 3);
    }

    #[test]
    fn find_subslice_locates_pattern() {
        assert_eq!(find_subslice(b"hello world", b"world"), Some(6));
        assert_eq!(find_subslice(b"hello", b"xyz"), None);
        assert_eq!(find_subslice(b"abc", b""), None);
    }
}
