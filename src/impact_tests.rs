use super::*;

#[test]
fn src_changes_select_unit_lane() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["src/main.rs".to_string()]);
    assert!(plan.selected_lanes.contains(&ImpactLane::Unit));
    assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
}

#[test]
fn config_changes_widen_to_full() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["Cargo.toml".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
    assert!(plan.widened_to_full);
}

#[test]
fn markdown_only_selects_docs_lane() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["README.md".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
    assert!(!plan.widened_to_full);
    assert!(plan.reason_codes.contains(&"docs_only_change".to_string()));
}

#[test]
fn multiple_markdown_files_select_docs_lane() {
    let plan = plan_from_changed_paths(
        1,
        "a",
        "b",
        vec![
            "README.md".to_string(),
            "docs/architecture.md".to_string(),
            "API.md".to_string(),
        ],
    );
    assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
    assert!(!plan.widened_to_full);
}

#[test]
fn markdown_plus_src_selects_unit_not_full() {
    let plan = plan_from_changed_paths(
        1,
        "a",
        "b",
        vec!["README.md".to_string(), "src/pool.rs".to_string()],
    );
    assert!(plan.selected_lanes.contains(&ImpactLane::Unit));
    assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
}

#[test]
fn rust_toolchain_triggers_full() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["rust-toolchain.toml".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
    assert!(plan.widened_to_full);
}

#[test]
fn cargo_dir_triggers_full() {
    let plan = plan_from_changed_paths(1, "a", "b", vec![".cargo/config.toml".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
    assert!(plan.widened_to_full);
}

#[test]
fn test_only_changes_select_integration() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["tests/pool_tests.rs".to_string()]);
    assert!(plan.selected_lanes.contains(&ImpactLane::Integration));
    assert!(!plan.selected_lanes.contains(&ImpactLane::Full));
}

#[test]
fn gitignore_selects_docs_not_full() {
    let plan = plan_from_changed_paths(1, "a", "b", vec![".gitignore".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::DocsOnly]);
}

#[test]
fn unknown_file_type_selects_non_code() {
    let plan = plan_from_changed_paths(1, "a", "b", vec!["data/fixture.json".to_string()]);
    assert_eq!(plan.selected_lanes, vec![ImpactLane::Full]);
    assert!(plan.widened_to_full);
    assert!(
        plan.reason_codes
            .contains(&"unknown_file_change".to_string())
    );
}
