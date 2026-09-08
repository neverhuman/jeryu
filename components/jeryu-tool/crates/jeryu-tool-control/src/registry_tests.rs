use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_root(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "jeryu-tool-registry-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join("tasks")).expect("create test root");
    root
}

fn copy_canonical(root: &Path) {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::copy(
        canonical.join("tools-registry.toml"),
        root.join("tools-registry.toml"),
    )
    .expect("copy registry");
    for entry in fs::read_dir(canonical.join("tasks")).expect("read tasks") {
        let entry = entry.expect("task entry");
        fs::copy(entry.path(), root.join("tasks").join(entry.file_name())).expect("copy task");
    }
}

#[test]
fn canonical_registry_is_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let summary = build_summary(&root).expect("canonical registry");
    assert!(summary["tool_count"].as_u64().unwrap_or(0) > 0);
    assert_eq!(
        summary["tools"].as_array().map(Vec::len),
        summary["tool_count"].as_u64().map(|value| value as usize)
    );
}

#[test]
fn registry_and_task_key_sets_are_closed() {
    let unknown_tool = test_root("unknown-tool");
    copy_canonical(&unknown_tool);
    let registry = fs::read_to_string(unknown_tool.join("tools-registry.toml"))
        .expect("read registry fixture")
        .replacen(
            "id = \"tuiwright\"",
            "id = \"tuiwright\"\nunknown_field = \"forbidden\"",
            1,
        );
    fs::write(unknown_tool.join("tools-registry.toml"), registry).expect("write hostile");
    assert!(build_summary(&unknown_tool).is_err());
    fs::remove_dir_all(unknown_tool).expect("remove fixture");

    let unknown_task = test_root("unknown-task");
    copy_canonical(&unknown_task);
    let task_path = unknown_task.join("tasks/0001-jeryu-ci-shell-lib.toml");
    let task = fs::read_to_string(&task_path)
        .expect("read task fixture")
        .replacen(
            "id = \"0001\"",
            "id = \"0001\"\nunknown_field = \"forbidden\"",
            1,
        );
    fs::write(&task_path, task).expect("write hostile task");
    assert!(build_summary(&unknown_task).is_err());
    fs::remove_dir_all(unknown_task).expect("remove fixture");
}
