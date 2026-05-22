use std::fs;
use std::path::PathBuf;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "how_to_code_rust-{name}-{}",
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(elapsed) => elapsed.as_nanos(),
            Err(_) => 0,
        }
    ));
    fs::create_dir_all(&path).expect("create scratch dir");
    path
}

#[test]
fn repair_bundle_is_noop_when_no_packets_exist() {
    let root = temp_dir("repair-noop");
    let bundle = build_repair_bundle(&root).expect("build repair bundle");
    assert_eq!(bundle.status, "no-failure");
    assert_eq!(bundle.failure_type, "no-failure");
    assert!(
        bundle
            .notes
            .iter()
            .any(|note| note.contains("compile failure"))
    );
}

#[test]
fn repair_bundle_skips_empty_compile_packets() {
    let root = temp_dir("repair-empty-compile");
    let agent_dir = root.join("target/agent");
    fs::create_dir_all(&agent_dir).expect("create agent dir");
    fs::write(
        agent_dir.join("compile-packets.json"),
        serde_json::to_string(&CompilePackets {
            generated_at: "2026-03-31".into(),
            packets: Vec::new(),
            summary: crate::model::CompileSummary {
                total_errors: 0,
                total_warnings: 0,
                arcs_affected: 0,
            },
        })
        .expect("serialize compile packets"),
    )
    .expect("write compile packets");

    let bundle = build_repair_bundle(&root).expect("build repair bundle");
    assert_eq!(bundle.status, "no-failure");
    assert_eq!(bundle.primary_arc, "<none>");
}
