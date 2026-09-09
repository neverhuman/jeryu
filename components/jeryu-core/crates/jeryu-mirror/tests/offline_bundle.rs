mod support;

use jeryu_mirror::{archive_from_github_value, read_bundle, verify_bundle, write_bundle};
use serde_json::json;

#[test]
fn offline_bundle_round_trips_and_verifies() {
    let path = support::temp_dir("bundle");
    let archive = archive_from_github_value(json!({
      "repositories": [{
        "owner": {"login": "acme"},
        "name": "rocket",
        "issues": [{"number": 1, "title": "bug", "state": "open"}]
      }]
    }))
    .unwrap();

    let manifest = write_bundle(&path, &archive).unwrap();
    assert_eq!(manifest.counts.repositories, 1);
    assert!(path.join("manifest.json").exists());
    assert!(path.join("repos/acme/rocket/issues.json").exists());

    let verification = verify_bundle(&path).unwrap();
    assert!(verification.ok, "{verification:?}");
    let restored = read_bundle(&path).unwrap();
    assert_eq!(restored.counts().issues, 1);
}

#[test]
fn offline_bundle_rejects_unknown_manifest_format() {
    let path = support::temp_dir("bundle-manifest-format");
    let archive = archive_from_github_value(json!({
      "repositories": [{
        "owner": {"login": "acme"},
        "name": "rocket"
      }]
    }))
    .unwrap();

    write_bundle(&path, &archive).unwrap();
    let manifest_path = path.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["format"] = json!("not-supported");
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let verification = verify_bundle(&path).unwrap();
    assert!(!verification.ok, "{verification:?}");
    assert!(
        verification
            .errors
            .iter()
            .any(|error| error.contains("manifest format is not supported"))
    );
}
