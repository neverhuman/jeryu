use serde_json::Value;
use std::process::Command;

#[test]
fn redline_contract_keeps_one_immutable_engine_identity() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .output()
        .expect("contract metadata");
    assert!(output.status.success(), "locked contract metadata failed");
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let redline: Vec<_> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"].as_str().unwrap().starts_with("redlinedb"))
        .collect();
    assert_eq!(redline.len(), 4);
    for package in redline {
        assert_eq!(
            package["source"],
            "git+https://github.com/neverhuman/redline-core.git?tag=redline-core-v4.1.0-jain.6#d0de59930141baffcfa2b514480e75b14627f24d"
        );
    }
}
