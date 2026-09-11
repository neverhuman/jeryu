use super::*;

const TOOLCHAIN: &str = "[toolchain]\nchannel = \"1.97.1\"\ncomponents = [\"rustfmt\", \"clippy\"]\nprofile = \"minimal\"\n";
const CONFIG: &str = "[build]\njobs = 2\n[net]\ngit-fetch-with-cli = true\n";
const COMPONENTS: [&str; 10] = [
    "jeryu-cache",
    "jeryu-ci-runner",
    "jeryu-core",
    "jeryu-deploy",
    "jeryu-intelligence",
    "jeryu-jira",
    "jeryu-release-ops",
    "jeryu-tool",
    "jeryu-tool-finder",
    "jeryu-web",
];

fn fixture() -> tempfile::TempDir {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    fs::create_dir(root.join(".cargo")).unwrap();
    fs::write(root.join("rust-toolchain.toml"), TOOLCHAIN).unwrap();
    fs::write(root.join(".cargo/config.toml"), CONFIG).unwrap();
    let mut manifest = String::from(
        "repo_family = \"jeryu-split\"\nrelease_lineage = \"v5\"\nstatus = \"candidate\"\nformal_ga = false\n\
        [handover]\nstatus = \"pending-protected-review\"\n\
        [storage]\ndefault_backend = \"sqlite\"\nbundled_sqlite = true\n\
        [redline]\nrole = \"optional-compatibility-proof\"\nrequired_for_release = false\n\
        contract_manifest = \"components/jeryu-release-ops/tests/redline/Cargo.toml\"\ntwo_consumer_proof_required = true\n\
        [[repo]]\nname = \"jeryu\"\npath = \".\"\nmirror_github_main = false\n",
    );
    for (index, component) in COMPONENTS.iter().enumerate() {
        let directory = root.join("components").join(component);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("rust-toolchain.toml"), TOOLCHAIN).unwrap();
        if index < 7 {
            fs::create_dir(directory.join(".cargo")).unwrap();
            fs::write(directory.join(".cargo/config.toml"), CONFIG).unwrap();
        }
        manifest.push_str(&format!(
            "[[repo]]\nname = \"{component}\"\npath = \"components/{component}\"\nmirror_github_main = false\n"
        ));
    }
    fs::write(root.join("repos.manifest.toml"), manifest).unwrap();
    temporary
}

#[test]
fn checks_every_component_and_preserves_inherited_configuration() {
    let temporary = fixture();
    let root = temporary.path();
    run(root, false).unwrap();
    assert_eq!(projections(root).unwrap().len(), 17);
    assert!(!root.join("components/jeryu-tool/.cargo").exists());
    let before = fs::metadata(root.join("components/jeryu-cache/rust-toolchain.toml")).unwrap();
    run(root, true).unwrap();
    assert!(!root.join("components/jeryu-tool/.cargo").exists());
    let after = fs::metadata(root.join("components/jeryu-cache/rust-toolchain.toml")).unwrap();
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(before.ino(), after.ino());
    }
}

#[test]
fn rejects_component_toolchain_and_build_override_drift() {
    for (path, replacement) in [
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.95.0\"\n"),
        (".cargo/config.toml", "[build]\njobs = 40\n"),
    ] {
        let temporary = fixture();
        let target = temporary.path().join("components/jeryu-deploy").join(path);
        fs::write(&target, replacement).unwrap();
        assert!(
            run(temporary.path(), false)
                .unwrap_err()
                .to_string()
                .contains("drift")
        );
        assert_eq!(fs::read_to_string(target).unwrap(), replacement);
    }
}

#[test]
fn renders_root_changes_byte_for_byte_and_is_idempotent() {
    let temporary = fixture();
    let root = temporary.path();
    let next_toolchain = TOOLCHAIN.replace("1.97.1", "1.97.2");
    let next_config = CONFIG.replace("jobs = 2", "jobs = 3");
    fs::write(root.join("rust-toolchain.toml"), &next_toolchain).unwrap();
    fs::write(root.join(".cargo/config.toml"), &next_config).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            root.join("components/jeryu-cache/rust-toolchain.toml"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    assert!(run(root, false).is_err());
    run(root, true).unwrap();
    for projection in projections(root).unwrap() {
        assert_eq!(projection.original, projection.desired);
        let expected = if projection.path.ends_with("rust-toolchain.toml") {
            &next_toolchain
        } else {
            &next_config
        };
        assert_eq!(fs::read_to_string(projection.path).unwrap(), *expected);
    }
    run(root, false).unwrap();
    run(root, true).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(root.join("components/jeryu-cache/rust-toolchain.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}

#[test]
fn invalid_root_inputs_cannot_partially_refresh_projections() {
    for (path, replacement) in [
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"stable\"\n"),
        ("rust-toolchain.toml", "[toolchain]\nchannel = \"1.97\"\n"),
        (
            "rust-toolchain.toml",
            "[toolchain]\nchannel = \"1.97.1\"\nchannel = \"1.95.0\"\n",
        ),
        (".cargo/config.toml", "[build"),
    ] {
        let temporary = fixture();
        let root = temporary.path();
        fs::write(root.join(path), replacement).unwrap();
        assert!(run(root, true).is_err());
        assert_eq!(
            fs::read_to_string(root.join("components/jeryu-cache/rust-toolchain.toml")).unwrap(),
            TOOLCHAIN
        );
        assert_eq!(
            fs::read_to_string(root.join("components/jeryu-cache/.cargo/config.toml")).unwrap(),
            CONFIG
        );
    }
}

#[test]
fn missing_projection_prevents_all_updates() {
    let temporary = fixture();
    let root = temporary.path();
    fs::write(
        root.join("rust-toolchain.toml"),
        TOOLCHAIN.replace("1.97.1", "1.97.2"),
    )
    .unwrap();
    fs::remove_file(root.join("components/jeryu-web/rust-toolchain.toml")).unwrap();
    assert!(run(root, true).is_err());
    assert_eq!(
        fs::read_to_string(root.join("components/jeryu-cache/rust-toolchain.toml")).unwrap(),
        TOOLCHAIN
    );
}

#[cfg(unix)]
#[test]
fn linked_sources_or_destinations_cannot_redirect_generation() {
    use std::os::unix::fs::symlink;
    for relative in [
        "rust-toolchain.toml",
        "components/jeryu-web/rust-toolchain.toml",
        "components/jeryu-cache/.cargo/config.toml",
    ] {
        let temporary = fixture();
        let root = temporary.path();
        let target = root.join(relative);
        let held = root.join("held-input.toml");
        fs::rename(&target, &held).unwrap();
        let bytes = fs::read(&held).unwrap();
        symlink(&held, &target).unwrap();
        assert!(run(root, true).is_err());
        assert_eq!(fs::read(&held).unwrap(), bytes);
        fs::remove_file(&target).unwrap();
        fs::hard_link(&held, &target).unwrap();
        assert!(run(root, true).is_err());
        assert_eq!(fs::read(&held).unwrap(), bytes);
    }
}

#[test]
fn changed_component_mapping_is_rejected_before_writes() {
    let temporary = fixture();
    let root = temporary.path();
    let manifest = fs::read_to_string(root.join("repos.manifest.toml")).unwrap();
    fs::write(
        root.join("repos.manifest.toml"),
        manifest.replace("path = \"components/jeryu-cache\"", "path = \"../outside\""),
    )
    .unwrap();
    assert!(run(root, true).is_err());
    assert_eq!(
        fs::read_to_string(root.join("components/jeryu-cache/rust-toolchain.toml")).unwrap(),
        TOOLCHAIN
    );
}

#[test]
fn legacy_rustup_and_cargo_names_cannot_override_the_root_files() {
    for directory in ["", "components/jeryu-cache"] {
        for name in ["rust-toolchain", ".cargo/config"] {
            let temporary = fixture();
            let root = temporary.path();
            let path = root.join(directory).join(name);
            fs::write(&path, "legacy override\n").unwrap();
            assert!(
                run(root, true)
                    .unwrap_err()
                    .to_string()
                    .contains("predecessor build configuration")
            );
            assert_eq!(
                fs::read_to_string(root.join("components/jeryu-cache/rust-toolchain.toml"))
                    .unwrap(),
                TOOLCHAIN
            );
            #[cfg(unix)]
            {
                fs::remove_file(&path).unwrap();
                std::os::unix::fs::symlink(root.join("missing"), &path).unwrap();
                assert!(run(root, true).is_err());
            }
        }
    }
}
