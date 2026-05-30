mod support;

use mirrorvault::{archive_from_github_value, read_bundle, verify_bundle, write_bundle};
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
