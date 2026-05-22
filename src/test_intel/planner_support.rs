use super::*;
use std::collections::BTreeSet;

pub(crate) fn compute_confidence(affected: &[&Subsystem], changed_paths: &[String]) -> f64 {
    if affected.is_empty() {
        return 0.0;
    }

    let mut confidence = 1.0;

    let has_cross_cutting = affected.iter().any(|s| s.cross_cutting);
    if has_cross_cutting {
        confidence -= 0.10;
    }

    if affected.len() > 3 {
        confidence -= 0.05 * (affected.len() as f64 - 3.0);
    }

    let matched_count = changed_paths
        .iter()
        .filter(|p| {
            affected
                .iter()
                .any(|s| subsystem::matches_any(p, s.owned_paths))
        })
        .count();
    let unmatched = changed_paths.len() - matched_count;
    if unmatched > 0 {
        confidence -= 0.15 * (unmatched as f64 / changed_paths.len() as f64);
    }

    confidence.clamp(0.0, 1.0)
}

pub(crate) fn dedup_selected_tests(tests: &mut Vec<SelectedTest>) {
    let mut seen = BTreeSet::new();
    tests.retain(|t| seen.insert(t.command.clone()));
}
