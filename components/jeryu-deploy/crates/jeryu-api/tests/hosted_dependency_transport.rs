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
    let members = metadata["workspace_members"].as_array().unwrap();
    let expected_external = if root.join(".jeryu-source.json").is_file() {
        let provenance: Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".jeryu-source.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(provenance["component"], "jeryu-deploy");
        assert_eq!(members.len(), 3);
        let source = provenance["source_commit"].as_str().unwrap();
        assert_eq!(source.len(), 40);
        assert!(source.bytes().all(|byte| byte.is_ascii_hexdigit()));
        Some(format!(
            "git+https://github.com/neverhuman/jeryu.git?rev={source}#{source}"
        ))
    } else {
        assert_eq!(internal.len(), 65);
        None
    };
    let mut names = std::collections::BTreeSet::new();
    for package in internal {
        assert!(
            names.insert(package["name"].as_str().unwrap()),
            "duplicate Jeryu package identity"
        );
        if members.contains(&package["id"]) {
            assert!(
                package["source"].is_null(),
                "workspace member must be local"
            );
            assert!(Path::new(package["manifest_path"].as_str().unwrap()).starts_with(root));
        } else {
            assert_eq!(
                package["source"].as_str(),
                expected_external.as_deref(),
                "external Jeryu dependency must bind the originating monorepo commit"
            );
            assert!(
                expected_external.is_some(),
                "Jeryu dependency escaped the monorepo"
            );
        }
    }
    let redline: Vec<_> = packages
        .iter()
        .filter(|p| p["name"].as_str().unwrap().starts_with("redlinedb"))
        .collect();
    if expected_external.is_some() {
        // Redline is an Obs dev-dependency. Cargo does not inherit the tests
        // of an external package into Deploy's standalone dependency closure.
        assert!(redline.is_empty());
        return;
    }
    assert_eq!(redline.len(), 4);
    let mut sources = std::collections::BTreeSet::new();
    for package in redline {
        let source = package["source"].as_str().expect("Redline is external");
        assert!(source.contains("tag=redline-core-v4.1.0-jain.6"));
        assert!(source.ends_with("#d0de59930141baffcfa2b514480e75b14627f24d"));
        sources.insert(source);
    }
    assert_eq!(sources.len(), 1, "Redline source identities diverged");
}
