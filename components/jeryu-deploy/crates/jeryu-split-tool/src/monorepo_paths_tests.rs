use super::*;
use std::os::unix::fs::symlink;

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("member")).unwrap();
    fs::write(directory.path().join("Cargo.toml"),
        "[workspace]\nmembers = ['member']\n[workspace.dependencies]\nalias = {package='owned', path='member'}\n").unwrap();
    fs::write(
        directory.path().join("member/Cargo.toml"),
        "[package]\nname = 'owned'\nversion = '1.0.0'\nworkspace = '..'\n",
    )
    .unwrap();
    directory
}

#[test]
fn owned_workspace_and_member_relative_paths_are_accepted() {
    let directory = fixture();
    check(directory.path()).unwrap();
    let manifest: Value =
        toml::from_str("[dependencies]\nalias = {package='owned', path='../member'}\n").unwrap();
    manifest_paths(
        directory.path(),
        &directory.path().join("member"),
        &manifest,
    )
    .unwrap();
}

#[test]
fn external_aliases_are_rejected_in_every_dependency_section() {
    let directory = fixture();
    for in_member in [false, true] {
        let source = if in_member {
            directory.path().join("member")
        } else {
            directory.path().to_path_buf()
        };
        for section in [
            "dependencies",
            "dev-dependencies",
            "build-dependencies",
            "workspace.dependencies",
            "target.'cfg(unix)'.dependencies",
            "target.'cfg(unix)'.dev-dependencies",
            "target.'cfg(unix)'.build-dependencies",
            "patch.'https://example.invalid/repo.git'",
            "replace",
        ] {
            // A non-Jeryu name must not escape checks based only on the name prefix.
            let escape = if in_member {
                "../../external"
            } else {
                "../external"
            };
            let manifest: Value = toml::from_str(&format!(
                "[{section}]\nordinary = {{package='serde', path='{escape}'}}\n"
            ))
            .unwrap();
            let error = manifest_paths(directory.path(), &source, &manifest).unwrap_err();
            assert!(
                format!("{error:#}").contains("dependency path"),
                "{section}: {error:#}"
            );
        }
    }
}

#[test]
fn missing_absolute_linked_and_malformed_dependency_paths_refuse() {
    let directory = fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    symlink(elsewhere.path(), directory.path().join("outside-link")).unwrap();
    symlink(
        directory.path().join("member"),
        directory.path().join("inside-link"),
    )
    .unwrap();
    for path in [
        "",
        "missing",
        "outside-link",
        "inside-link",
        "/tmp",
        "inside-link/../member",
        "outside-link/../member",
        "missing/../member",
    ] {
        let manifest: Value =
            toml::from_str(&format!("[dependencies]\na = {{path='{path}'}}\n")).unwrap();
        assert!(
            manifest_paths(directory.path(), directory.path(), &manifest).is_err(),
            "{path}"
        );
    }
    for source in [
        "[dependencies]\na = {path=42}",
        "dependencies = []",
        "target = []",
        "[target]\nx = 42",
        "patch = []",
        "[patch]\nx = []",
        "workspace = 4",
        "[package]\nworkspace = 'member'",
    ] {
        let manifest: Value = toml::from_str(source).unwrap();
        assert!(
            manifest_paths(directory.path(), directory.path(), &manifest).is_err(),
            "{source}"
        );
    }
}

#[test]
fn actual_manifest_discovery_refuses_external_duplicate_and_linked_members() {
    let directory = fixture();
    for member in ["../elsewhere", "missing", "member/*"] {
        fs::write(
            directory.path().join("Cargo.toml"),
            format!("[workspace]\nmembers=['{member}']\n"),
        )
        .unwrap();
        assert!(check(directory.path()).is_err(), "{member}");
    }
    fs::write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers=['member','./member']\n",
    )
    .unwrap();
    assert!(check(directory.path()).is_err());
    fs::write(
        directory.path().join("Cargo.toml"),
        "[workspace]\nmembers=['member']\n",
    )
    .unwrap();
    let member = directory.path().join("member/Cargo.toml");
    fs::rename(&member, directory.path().join("retained.toml")).unwrap();
    symlink(directory.path().join("retained.toml"), &member).unwrap();
    assert!(check(directory.path()).is_err());
}
