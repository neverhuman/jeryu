//! Shared path-matching helper used by the file-path-driven detectors.
//!
//! Reads only `EvidencePack::changed_files` — never the network, filesystem,
//! git, or an LLM.

use crate::types::EvidencePack;

/// Returns the changed-file paths that match any of `exact` (exact path) or
/// `prefixes` (path prefix), or `None` if none match.
pub(super) fn any_path_matches(
    p: &EvidencePack,
    exact: &[&str],
    prefixes: &[&str],
) -> Option<Vec<String>> {
    let hits: Vec<String> = p
        .changed_files
        .iter()
        .filter(|f| {
            exact.iter().any(|e| f.path == *e)
                || prefixes.iter().any(|prefix| f.path.starts_with(prefix))
        })
        .map(|f| f.path.clone())
        .collect();
    (!hits.is_empty()).then_some(hits)
}
