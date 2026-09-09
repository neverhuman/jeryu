use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const TAG: &str = "v1.6.11-deadlang-precision-split.3";
const REV: &str = "b88562fdb124aa86dedd70ab972e7d0d87e58be1";
const TREE: &str = "611229e54938c0e8808896e369fd54d095d258f7";
const ARCHIVE_SHA256: &str = "903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa";
const BINARY_SHA256: &str = "9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c";
const MANIFEST_COMMIT: &str = "e8218f9f39bf38277646f31daf2f2ee56e9a7eff";
const MANIFEST_TREE: &str = "dae370c78d9e71123bf239ef06add4ee4d04e34e";
const MANIFEST_SHA256: &str = "be001dc52c66da5669167f3e429d882184931baa3d7a0e53b605c17425872b5a";
const IMAGE_RECEIPT_SHA256: &str =
    "5e936e6c062cc6c4ef697e96b0fd1fd136ede04802667b191ac3059ac37b1d16";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let root = repository_root();
    let path = if let Some(workflow) = relative.strip_prefix(".github/workflows/")
        && root.join(".jeryu-source.json").is_file()
    {
        assert!(
            !root.join(relative).exists(),
            "historical workflow must remain inactive in a split export"
        );
        assert!(root.join(".github/workflows/split.yml").is_file());
        root.join("docs/split-original-workflows").join(workflow)
    } else {
        root.join(relative)
    };
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn proptest_equivalent_generated_jankurai_consumers_share_one_closed_identity() {
    let full_identity_consumers = [
        ".github/workflows/ci-fast.yml",
        ".github/workflows/jankurai.yml",
        ".github/workflows/proof-evidence.yml",
        ".github/workflows/release.yml",
        ".github/workflows/security.yml",
        "crates/jeryu-api/src/ci_bridge.rs",
        "images/agent-sandbox/Dockerfile",
        "images/agent-sandbox/jankurai-installation-receipt.json",
        "ops/agent-sandbox/smoke.sh",
        "ops/ci/common.sh",
        "ops/ci/ensure-jankurai.sh",
        "ops/ci/lib.sh",
        "ops/ci/pr-ci.sh",
        "scripts/ci-doctor.sh",
    ];
    let identity_properties = [TAG, REV, TREE, ARCHIVE_SHA256, BINARY_SHA256];

    for relative in full_identity_consumers {
        let content = read(relative);
        for expected in identity_properties {
            assert!(
                content.contains(expected),
                "{relative} omitted governed identity property {expected}"
            );
        }
    }

    let active_projection = [
        ".github/workflows/ci-fast.yml",
        ".github/workflows/jankurai.yml",
        ".github/workflows/proof-evidence.yml",
        ".github/workflows/release.yml",
        ".github/workflows/security.yml",
        "CHANGELOG.md",
        "agent/native-cli-manifest.toml",
        "crates/jeryu-api/src/ci_bridge.rs",
        "docs/release.md",
        "images/agent-sandbox/Dockerfile",
        "images/agent-sandbox/README.md",
        "images/agent-sandbox/jankurai-installation-receipt.json",
        "ops/agent-sandbox/smoke.sh",
        "ops/ci/common.sh",
        "ops/ci/ensure-jankurai.sh",
        "ops/ci/lib.sh",
        "ops/ci/pr-ci.sh",
        "ops/ci/test-governed-jankurai.sh",
        "scripts/ci-doctor.sh",
    ];
    let retired_identity = [
        "v1.6.11-deadlang-precision-split.1",
        "dface7397fe24d46b0b1885ddd5782c34edbff49",
        "34a8a1fb59bc4ebfadf12c45d95f169d06acc781",
        "2fbca5d04083e3c8d32f383d5b6b4520b8911690b26968c6fbcb210e1202b938",
        "fdb42e5fa7d9851c0729e59bf1e582c895aa9cfc03a7175b420c6025d2fd014e",
    ];

    for relative in active_projection {
        let content = read(relative);
        for retired in retired_identity {
            assert!(
                !content.contains(retired),
                "{relative} retains retired active identity {retired}"
            );
        }
    }
}

#[test]
fn integration_receipt_and_release_broker_contract_remain_fail_closed() {
    let receipt: serde_json::Value = serde_json::from_str(&read(
        "images/agent-sandbox/jankurai-installation-receipt.json",
    ))
    .expect("installation receipt must be valid JSON");

    assert_eq!(receipt["binary"]["sha256"], BINARY_SHA256);
    assert_eq!(receipt["source"]["tag"], TAG);
    assert_eq!(receipt["source"]["commit"], REV);
    assert_eq!(receipt["source"]["tree"], TREE);
    assert_eq!(receipt["source"]["archive_sha256"], ARCHIVE_SHA256);
    assert_eq!(receipt["governance"]["manifest_commit"], MANIFEST_COMMIT);
    assert_eq!(receipt["governance"]["manifest_tree"], MANIFEST_TREE);
    assert_eq!(receipt["governance"]["manifest_sha256"], MANIFEST_SHA256);
    assert_eq!(receipt["governance"]["protected_main"], true);
    assert_eq!(receipt["governance"]["status"], "governed");
    assert_eq!(receipt["conclusion"], "success");
    assert_eq!(receipt["test_mode"], false);

    for relative in ["ops/ci/ensure-jankurai.sh", "ops/ci/lib.sh"] {
        let verifier = read(relative);
        for required in [
            "mode=release-broker",
            "/opt/jain-ci/authority/release-bin/jankurai",
            "expected mode 0555 and one link",
            "release broker Jankurai rejects caller receipt authority",
        ] {
            assert!(
                verifier.contains(required),
                "{relative} omitted broker custody invariant {required}"
            );
        }
    }

    let image = read("images/agent-sandbox/Dockerfile");
    let smoke = read("ops/agent-sandbox/smoke.sh");
    assert!(image.contains(IMAGE_RECEIPT_SHA256));
    assert!(smoke.contains(IMAGE_RECEIPT_SHA256));
}

#[test]
fn monorepo_dependencies_share_the_root_workspace_and_lock() {
    let root = repository_root().join("../..");
    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
    assert!(
        cargo.get("patch").is_none(),
        "monorepo must not need source identity patches"
    );
    let api: toml::Value = toml::from_str(&read("crates/jeryu-api/Cargo.toml")).unwrap();
    let dependencies = api["dependencies"].as_table().unwrap();
    for (name, dependency) in dependencies {
        if !name.starts_with("jeryu-") {
            continue;
        }
        assert_eq!(
            dependency["workspace"].as_bool(),
            Some(true),
            "{name} must inherit its source"
        );
        assert!(dependency.get("git").is_none() && dependency.get("path").is_none());
        let path = cargo["workspace"]["dependencies"][name]["path"]
            .as_str()
            .unwrap();
        assert!(path.starts_with("components/") && !path.contains(".."));
        assert!(root.join(path).join("Cargo.toml").is_file());
    }
    let lock: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.lock")).unwrap()).unwrap();
    let mut counts = BTreeMap::new();
    for package in lock["package"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if name.starts_with("jeryu-") {
            *counts.entry(name).or_insert(0) += 1;
            assert!(
                package.get("source").is_none(),
                "{name} must have one local source"
            );
        }
    }
    assert_eq!(counts.len(), 65);
    assert!(counts.values().all(|count| *count == 1));
}
