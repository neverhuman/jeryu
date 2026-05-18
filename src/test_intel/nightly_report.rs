use super::nightly::AuditReport;

/// Human-readable audit report.
pub fn explain_audit(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str("╭─ VTI Nightly Oracle Audit ────────────────────╮\n");
    out.push_str(&format!(
        "│ SHA:      {:<36} │\n",
        &report.nightly_sha[..8.min(report.nightly_sha.len())]
    ));
    out.push_str(&format!("│ Tests:    {:<36} │\n", report.total_tests));
    out.push_str(&format!(
        "│ Failed:   {:<36} │\n",
        report.failed_tests.len()
    ));
    out.push_str(&format!("│ Misses:   {:<36} │\n", report.misses.len()));
    out.push_str(&format!(
        "│ Accuracy: {:<36.1}% │\n",
        report.accuracy * 100.0
    ));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    if report.full_run_clean {
        out.push_str("✅ Full nightly run was clean — no misses possible.\n\n");
    }

    if !report.misses.is_empty() {
        out.push_str("Selector misses:\n");
        for miss in &report.misses {
            out.push_str(&format!(
                "  ❌ {} (subsystem: {})\n",
                miss.missed_test,
                miss.responsible_subsystem.as_deref().unwrap_or("unknown")
            ));
        }
        out.push('\n');
    }

    if !report.vti_selected.is_empty() {
        out.push_str("VTI would have selected:\n");
        for cmd in &report.vti_selected {
            out.push_str(&format!("  ✓ {}\n", cmd));
        }
        out.push('\n');
    }

    if !report.vti_skipped.is_empty() {
        out.push_str("VTI would have skipped:\n");
        for sub in &report.vti_skipped {
            out.push_str(&format!("  ○ {}\n", sub));
        }
    }

    out
}

/// JSON representation of an audit report.
pub fn explain_audit_json(report: &AuditReport) -> serde_json::Value {
    serde_json::json!({
        "nightly_sha": report.nightly_sha,
        "total_tests": report.total_tests,
        "failed_tests": report.failed_tests,
        "vti_selected": report.vti_selected,
        "vti_skipped": report.vti_skipped,
        "misses": report.misses,
        "accuracy": report.accuracy,
        "full_run_clean": report.full_run_clean,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_intel::nightly::{audit_selector, learn_from_audit};

    #[test]
    fn clean_nightly_no_misses() {
        let report = audit_selector(
            &["src/pool.rs".into()],
            &[], // no failures
            &["test_pool".into(), "test_cache".into()],
            "abc123",
            None,
        );
        assert!(report.full_run_clean);
        assert!(report.misses.is_empty());
        assert_eq!(report.accuracy, 1.0);
    }

    #[test]
    fn covered_failure_not_a_miss() {
        let report = audit_selector(
            &["src/pool.rs".into()],
            &["pool_connection_test".into()],
            &["pool_connection_test".into(), "cache_hit_test".into()],
            "abc123",
            None,
        );
        assert!(report.misses.is_empty());
    }

    #[test]
    fn uncovered_failure_is_a_miss() {
        let report = audit_selector(
            &["src/tui/ui.rs".into()],
            &["cache_eviction_test".into()],
            &["tui_render_test".into(), "cache_eviction_test".into()],
            "abc123",
            None,
        );
        assert_eq!(report.misses.len(), 1);
        assert_eq!(report.misses[0].missed_test, "cache_eviction_test");
        assert!(report.accuracy < 1.0);
    }

    #[test]
    fn learn_from_clean_audit() {
        let report = audit_selector(
            &["src/pool.rs".into()],
            &[],
            &["test1".into()],
            "abc123",
            None,
        );
        let result = learn_from_audit(&report);
        assert_eq!(result.new_misses, 0);
        assert!(result.suggestions[0].contains("No selector misses"));
    }

    #[test]
    fn learn_from_miss_suggests_fix() {
        let report = audit_selector(
            &["src/tui/ui.rs".into()],
            &["cache_eviction_test".into()],
            &["tui_test".into(), "cache_eviction_test".into()],
            "abc12345",
            None,
        );
        let result = learn_from_audit(&report);
        assert_eq!(result.new_misses, 1);
        assert!(!result.flagged_subsystems.is_empty());
        assert!(result.suggestions.iter().any(|s| s.contains("widening")));
    }

    #[test]
    fn explain_formats_correctly() {
        let report = audit_selector(
            &["src/tui/ui.rs".into()],
            &["cache_eviction_test".into()],
            &["tui_test".into(), "cache_eviction_test".into()],
            "abc12345deadbeef",
            None,
        );
        let text = explain_audit(&report);
        assert!(text.contains("Oracle Audit"));
        assert!(text.contains("cache_eviction_test"));
        assert!(text.contains("Selector misses"));
    }

    #[test]
    fn audit_json_contains_key_fields() {
        let report = audit_selector(
            &["src/pool.rs".into()],
            &[],
            &["test1".into()],
            "abc123",
            None,
        );
        let json = explain_audit_json(&report);
        assert_eq!(json["full_run_clean"], true);
        assert_eq!(json["accuracy"], 1.0);
    }
}
