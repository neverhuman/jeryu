use crate::test_intel::nightly::{audit_selector, learn_from_audit};
use crate::test_intel::nightly_report::{explain_audit, explain_audit_json};

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
