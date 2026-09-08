//! Source identity proof for the consolidated workspace.

use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn jeryu_packages_have_one_workspace_identity_and_redline_stays_immutable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join("Cargo.lock").is_file())
        .expect("workspace lock");
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .output()
        .expect("cargo metadata");
    assert!(output.status.success(), "locked metadata failed");
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let internal: Vec<_> = packages
        .iter()
        .filter(|p| p["name"].as_str().unwrap().starts_with("jeryu-"))
        .collect();
    assert_eq!(internal.len(), 65);
    let mut names = std::collections::BTreeSet::new();
    for package in internal {
        assert!(
            names.insert(package["name"].as_str().unwrap()),
            "duplicate Jeryu package identity"
        );
        assert!(
            package["source"].is_null(),
            "Jeryu dependency escaped the workspace"
        );
        assert!(Path::new(package["manifest_path"].as_str().unwrap()).starts_with(root));
    }
    let redline: Vec<_> = packages
        .iter()
        .filter(|p| p["name"].as_str().unwrap().starts_with("redlinedb"))
        .collect();
    assert!(!redline.is_empty());
    let mut sources = std::collections::BTreeSet::new();
    for package in redline {
        let source = package["source"].as_str().expect("Redline is external");
        assert!(source.contains("tag=redline-core-v4.1.0-jain.6"));
        assert!(source.ends_with("#d0de59930141baffcfa2b514480e75b14627f24d"));
        sources.insert(source);
    }
    assert_eq!(sources.len(), 1, "Redline source identities diverged");
}
