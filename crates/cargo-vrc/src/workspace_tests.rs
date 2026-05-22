use super::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_path(prefix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock moved backwards")
        .as_nanos();
    workspace_root()
        .expect("workspace root")
        .join("target")
        .join("cargo-vrc-tests")
        .join(format!("{prefix}-{stamp}"))
}

#[test]
fn normalize_workspace_path_accepts_paths_inside_root() {
    let root = unique_path("workspace-root");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create nested directory");
    let file = nested.join("Cargo.toml");
    fs::write(&file, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n").expect("write manifest");

    let root = fs::canonicalize(&root).expect("canonicalize root");
    let normalized = normalize_workspace_path(&root, &file).expect("normalize path");
    assert!(normalized.starts_with(&root));

    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn normalize_manifest_path_requires_cargo_toml() {
    let root = unique_path("workspace-manifest");
    fs::create_dir_all(&root).expect("create root directory");
    let manifest = root.join("not-a-manifest.txt");
    fs::write(&manifest, "manifest fixture").expect("write manifest-like file");

    let err = normalize_manifest_path(&manifest).expect_err("reject non-manifest path");
    assert!(err.to_string().contains("Cargo.toml"));

    let _ = fs::remove_file(&manifest);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn normalize_manifest_path_rejects_traversal_outside_workspace() {
    // Negative test for HLT-023-INPUT-BOUNDARY-GAP: confirm that a
    // user-supplied manifest path that canonicalizes outside the workspace
    // is refused before it can reach `MetadataCommand::exec`.
    //
    // The OS-provided ephemeral directory canonicalizes to a location
    // outside the compile-time workspace root on macOS (e.g.
    // /private/var/...), so a manifest written there is rejected by the
    // allowlist enforced by `normalize_manifest_path`.
    let outside_root = std::env::temp_dir().join(format!(
        "cargo-vrc-outside-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock moved backwards")
            .as_nanos()
    ));
    fs::create_dir_all(&outside_root).expect("create outside root");
    let outside_manifest = outside_root.join("Cargo.toml");
    fs::write(
        &outside_manifest,
        "[package]\nname = \"hostile\"\nversion = \"0.0.0\"\n",
    )
    .expect("write outside manifest");

    let err =
        normalize_manifest_path(&outside_manifest).expect_err("reject manifest outside workspace");
    let message = err.to_string();
    assert!(
        message.contains("escapes workspace root"),
        "unexpected error: {message}"
    );

    let _ = fs::remove_file(&outside_manifest);
    let _ = fs::remove_dir_all(&outside_root);
}

#[test]
fn normalize_workspace_path_rejects_paths_outside_root() {
    let root = unique_path("workspace-root");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create nested directory");
    let inside = nested.join("Cargo.toml");
    fs::write(&inside, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n")
        .expect("write manifest");

    let outside = unique_path("workspace-outside.toml");
    fs::write(
        &outside,
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("write outside manifest");

    let root = fs::canonicalize(&root).expect("canonicalize root");
    let err = normalize_workspace_path(&root, &outside).expect_err("reject outside path");
    assert!(err.to_string().contains("escapes workspace root"));

    let _ = fs::remove_file(&inside);
    let _ = fs::remove_file(&outside);
    let _ = fs::remove_dir_all(&root);
}
