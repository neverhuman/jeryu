//! Source identity proof for the consolidated workspace.

use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn jeryu_packages_have_one_workspace_identity_and_sqlite_needs_no_redline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|path| path.join("Cargo.lock").is_file())
        .expect("workspace lock");
    let output = Command::new("cargo")
        .current_dir(root)
        .env_remove("CARGO")
        .env_remove("CARGO_PRIMARY_PACKAGE")
        .env_remove("CARGO_MANIFEST_DIR")
        .args([
            "metadata",
            "--manifest-path",
            root.join("Cargo.toml").to_str().expect("utf8 root"),
            "--locked",
            "--all-features",
            "--format-version",
            "1",
        ])
        .output()
        .expect("cargo metadata");
    assert!(
        output.status.success(),
        "locked metadata failed: {}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
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
    assert!(
        redline.is_empty(),
        "SQLite builds must not resolve Redline, even with all features"
    );
    let sqlite: Vec<_> = packages
        .iter()
        .filter(|p| p["name"] == "libsqlite3-sys")
        .collect();
    assert_eq!(sqlite.len(), 1, "one SQLite implementation");
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == sqlite[0]["id"])
        .unwrap();
    assert!(
        node["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "bundled"),
        "SQLite must build without a host database installation"
    );
}
