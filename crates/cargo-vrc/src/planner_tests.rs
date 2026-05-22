use super::*;
use crate::load_workspace;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn current_manifest() -> PathBuf {
    workspace_root().join("Cargo.toml")
}

#[test]
fn crate_local_change_stays_local() {
    let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
    let plan = build_vrc_plan(&snapshot, &[PathBuf::from("crates/cargo-vrc/src/model.rs")])
        .expect("build vrc plan");

    assert_eq!(plan.selected_arcs.len(), 1);
    assert_eq!(plan.selected_arcs[0].name, "cargo-vrc");
    assert_eq!(
        plan.stop_condition,
        "stop after local compile, tests, and doctests"
    );
    assert!(!plan.selected_tests.is_empty());
}

#[test]
fn explain_subject_returns_package_match() {
    let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
    let explanation = explain_subject(&snapshot, "cargo-vrc").expect("explain subject");

    let matches = explanation["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["arc"], "cargo-vrc");
}

#[test]
fn current_workspace_maps_use_stable_relative_paths() {
    let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");
    let agent_map = build_agent_map(&snapshot);
    let test_map = build_test_map(&snapshot);

    assert_eq!(agent_map.workspace_root, ".");
    assert_eq!(test_map.workspace_root, ".");
    assert!(
        agent_map
            .members
            .iter()
            .all(|member| !member.manifest_path.starts_with('/'))
    );
    assert!(
        agent_map
            .members
            .iter()
            .all(|member| !member.package_root.starts_with('/'))
    );

    let harnesses = test_map
        .entries
        .iter()
        .flat_map(|entry| entry.integration_harnesses.iter())
        .collect::<Vec<_>>();
    assert!(!harnesses.is_empty());
    assert!(harnesses.iter().all(|path| !path.starts_with('/')));
    assert!(harnesses.iter().all(|path| !path.contains("/../")));
}

#[test]
fn current_workspace_root_paths_select_jeryu_only() {
    let snapshot = load_workspace(Some(&current_manifest())).expect("load current workspace");

    let plan =
        build_vrc_plan(&snapshot, &[PathBuf::from("src/admission.rs")]).expect("root source plan");
    assert!(
        plan.selected_arcs.iter().any(|arc| arc.name == "jeryu"),
        "root src changes should select jeryu"
    );
    assert!(
        !plan.selected_arcs.iter().any(|arc| arc.name == "cargo-vrc"),
        "package-local src/** globs must not match root src changes"
    );
}
