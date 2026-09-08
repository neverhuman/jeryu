use std::path::{Path, PathBuf};

const PACKAGE: &str = "jeryu-rustjet";
const VERSION: &str = "5.0.0";
const REPOSITORY: &str = "http://127.0.0.1:8787/git/jeryu/jeryu-intelligence.git";
const TAG: &str = "jeryu-intelligence-v5.0.0-split.0";
const COMMIT: &str = "de5b22d50e21b9cac3efd239ea2ec2eda1df6327";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate must be nested beneath the workspace root")
        .to_owned()
}

fn string_field<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str, String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{PACKAGE} must define string field `{key}`"))
}

fn validate_source_contract(manifest: &str, lock: &str) -> Result<(), String> {
    let manifest: toml::Value = manifest
        .parse()
        .map_err(|error| format!("invalid manifest TOML: {error}"))?;
    let dependency = manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get(PACKAGE))
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("missing table dependency `{PACKAGE}`"))?;

    let mut keys: Vec<_> = dependency.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["git", "package", "tag"] {
        return Err(format!(
            "{PACKAGE} dependency must contain exactly git/package/tag, found {keys:?}"
        ));
    }
    if string_field(dependency, "git")? != REPOSITORY {
        return Err(format!(
            "{PACKAGE} must use the canonical loopback local-forge repository"
        ));
    }
    if string_field(dependency, "tag")? != TAG {
        return Err(format!("{PACKAGE} must use exact immutable tag {TAG}"));
    }
    if string_field(dependency, "package")? != PACKAGE {
        return Err(format!("{PACKAGE} package identity must not change"));
    }

    let lock: toml::Value = lock
        .parse()
        .map_err(|error| format!("invalid lock TOML: {error}"))?;
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "Cargo.lock is missing package records".to_owned())?;
    let matches: Vec<_> = packages
        .iter()
        .filter_map(toml::Value::as_table)
        .filter(|package| package.get("name").and_then(toml::Value::as_str) == Some(PACKAGE))
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "Cargo.lock must contain exactly one {PACKAGE} identity, found {}",
            matches.len()
        ));
    }

    let package = matches[0];
    if string_field(package, "version")? != VERSION {
        return Err(format!("{PACKAGE} must remain version {VERSION}"));
    }
    let expected_source = format!("git+{REPOSITORY}?tag={TAG}#{COMMIT}");
    if string_field(package, "source")? != expected_source {
        return Err(format!(
            "{PACKAGE} lock source must bind the canonical repository, exact tag, and full commit"
        ));
    }
    Ok(())
}

fn valid_manifest() -> String {
    format!(
        "[dependencies]\n{PACKAGE} = {{ git = \"{REPOSITORY}\", tag = \"{TAG}\", package = \"{PACKAGE}\" }}\n"
    )
}

fn valid_lock() -> String {
    format!(
        "version = 4\n\n[[package]]\nname = \"{PACKAGE}\"\nversion = \"{VERSION}\"\nsource = \"git+{REPOSITORY}?tag={TAG}#{COMMIT}\"\n"
    )
}

#[test]
fn repository_manifest_and_lock_use_exact_local_forge_identity() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(root.join("crates/jeryu-wsversion/Cargo.toml"))
        .expect("read jeryu-wsversion manifest");
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("read workspace lock");
    validate_source_contract(&manifest, &lock).expect("validate governed Cargo source identity");
}

#[test]
fn hostile_source_identities_fail_closed() {
    let manifest = valid_manifest();
    let lock = valid_lock();
    validate_source_contract(&manifest, &lock).expect("canonical fixture must pass");

    let hostile_manifests = [
        manifest.replace(
            REPOSITORY,
            "https://github.com/neverhuman/jeryu-intelligence.git",
        ),
        manifest.replace(
            REPOSITORY,
            "http://localhost:8787/git/jeryu/jeryu-intelligence.git",
        ),
        manifest.replace(&format!(", tag = \"{TAG}\""), ""),
        manifest.replace(&format!("tag = \"{TAG}\""), &format!("rev = \"{COMMIT}\"")),
        manifest.replace(TAG, "jeryu-intelligence-v5.0.0-split.1"),
        manifest.replace(
            &format!("package = \"{PACKAGE}\""),
            &format!("package = \"{PACKAGE}\", branch = \"main\""),
        ),
    ];
    for hostile in hostile_manifests {
        assert!(
            validate_source_contract(&hostile, &lock).is_err(),
            "hostile manifest unexpectedly passed: {hostile}"
        );
    }

    let hostile_locks = [
        lock.replace(
            REPOSITORY,
            "https://github.com/neverhuman/jeryu-intelligence.git",
        ),
        lock.replace(
            REPOSITORY,
            "http://localhost:8787/git/jeryu/jeryu-intelligence.git",
        ),
        lock.replace(TAG, "jeryu-intelligence-v5.0.0-split.1"),
        lock.replace(COMMIT, &COMMIT[..12]),
        lock.replace(COMMIT, "0000000000000000000000000000000000000000"),
        format!(
            "{lock}\n[[package]]\nname = \"{PACKAGE}\"\nversion = \"{VERSION}\"\nsource = \"git+https://github.com/neverhuman/jeryu-intelligence.git?tag={TAG}#{COMMIT}\"\n"
        ),
    ];
    for hostile in hostile_locks {
        assert!(
            validate_source_contract(&manifest, &hostile).is_err(),
            "hostile lock unexpectedly passed: {hostile}"
        );
    }
}
