//! Port of `scripts/check-docs.py`: required docs exist and contain required markers.

use std::path::Path;

use anyhow::{Context, Result};

use crate::report::GateReport;

/// Required documents and the substrings (markers) each must contain, ported
/// verbatim from `check-docs.py`.
pub const REQUIRED_DOCS: [(&str, &[&str]); 4] = [
    ("README.md", &["Jeryu", "JeryuCache", "Phase 12"]),
    (
        "docs/engineering_spec.md",
        &[
            "Jeryu Engineering Spec",
            "Cache correctness beats cache hit rate",
        ],
    ),
    ("docs/PHASE12_SPEC.md", &["Phase 12", "Zero false hits"]),
    ("docs/RUNBOOKS.md", &["runbooks"]),
];

/// Port of `scripts/check-docs.py`.
///
/// For each required doc (resolved relative to `base`), checks the file exists
/// and contains every required marker substring.
///
/// # Errors
/// Returns an error only if a file that exists cannot be read as UTF-8 text. A
/// missing file or a missing marker is a normal gate failure (not an error),
/// mirroring the Python behavior.
pub fn docs(base: &Path) -> Result<GateReport> {
    let mut missing = Vec::new();
    for (rel, needles) in REQUIRED_DOCS {
        let path = base.join(rel);
        if !path.exists() {
            missing.push(format!("missing required doc: {rel}"));
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        for needle in needles {
            if !text.contains(needle) {
                missing.push(format!("{rel} missing marker: {needle}"));
            }
        }
    }

    if missing.is_empty() {
        Ok(GateReport::pass("docs ok"))
    } else {
        Ok(GateReport::fail(missing))
    }
}
