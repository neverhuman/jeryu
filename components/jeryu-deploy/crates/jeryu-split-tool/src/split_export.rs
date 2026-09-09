//! Standalone dependency projections and lock validation for split previews.

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::Path, process::Command};

use crate::monorepo::git;

pub(super) fn ci_files(component: &str) -> BTreeMap<String, String> {
    let sandbox = if component == "jeryu-ci-runner" {
        "true"
    } else {
        "false"
    };
    BTreeMap::from([
        (
            "scripts/split-ci.sh".into(),
            include_str!("split_ci.sh").into(),
        ),
        (
            ".github/workflows/split.yml".into(),
            include_str!("split_ci.yml")
                .replace("@COMPONENT@", component)
                .replace("@SANDBOX@", sandbox),
        ),
    ])
}

pub(super) fn npm_files(
    root: &Path,
    source: &str,
    prefix: &str,
) -> Result<BTreeMap<String, String>> {
    let package: Value = serde_json::from_str(&git(
        root,
        &["show", &format!("{source}:package.json")],
        None,
    )?)?;
    let lock: Value = serde_json::from_str(&git(
        root,
        &["show", &format!("{source}:package-lock.json")],
        None,
    )?)?;
    let (package, lock) = project_npm(package, lock, prefix)?;
    Ok(BTreeMap::from([
        (
            "package.json".into(),
            format!("{}\n", serde_json::to_string_pretty(&package)?),
        ),
        (
            "package-lock.json".into(),
            format!("{}\n", serde_json::to_string_pretty(&lock)?),
        ),
    ]))
}

fn project_npm(mut package: Value, mut lock: Value, prefix: &str) -> Result<(Value, Value)> {
    let prefix = format!("{prefix}/");
    let workspaces = package["workspaces"]
        .as_array_mut()
        .context("npm workspaces")?;
    for workspace in workspaces.iter_mut() {
        *workspace = Value::String(
            workspace
                .as_str()
                .and_then(|path| path.strip_prefix(&prefix))
                .context("npm workspace belongs to another component")?
                .to_owned(),
        );
    }
    package["name"] = Value::String("jeryu-web".into());
    for command in package["scripts"]
        .as_object_mut()
        .context("npm scripts")?
        .values_mut()
    {
        *command = Value::String(
            command
                .as_str()
                .context("npm command")?
                .replace(&prefix, ""),
        );
    }
    package["scripts"]["security"] = Value::String("npm audit --audit-level=high".into());
    let mut packages = serde_json::Map::new();
    for (path, mut entry) in lock["packages"]
        .as_object()
        .context("npm lock packages")?
        .clone()
    {
        if entry["link"].as_bool() == Some(true) {
            entry["resolved"] = Value::String(
                entry["resolved"]
                    .as_str()
                    .and_then(|path| path.strip_prefix(&prefix))
                    .context("workspace link escapes component")?
                    .into(),
            );
        }
        let path = path.strip_prefix(&prefix).unwrap_or(&path).to_owned();
        ensure!(
            packages.insert(path, entry).is_none(),
            "npm lock path collision"
        );
    }
    packages.get_mut("").context("npm root package")?["name"] = package["name"].clone();
    packages.get_mut("").unwrap()["workspaces"] = package["workspaces"].clone();
    lock["name"] = package["name"].clone();
    lock["packages"] = Value::Object(packages);
    Ok((package, lock))
}

/// Cargo/npm operate on an exact export commit in an automatically removed clone.
/// This returns a lock, never a publication or anonymous-build attestation.
pub(super) fn resolve_lock(root: &Path, tree: &str, source: &str, npm: bool) -> Result<String> {
    let root = root.canonicalize()?;
    let output = Command::new("git")
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "Jeryu split export")
        .env("GIT_AUTHOR_EMAIL", "split@jeryu.invalid")
        .env("GIT_COMMITTER_NAME", "Jeryu split export")
        .env("GIT_COMMITTER_EMAIL", "split@jeryu.invalid")
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit-tree",
            tree,
            "-m",
            "Disposable split lock validation",
        ])
        .output()?;
    ensure!(
        output.status.success(),
        "cannot create disposable export commit"
    );
    let commit = String::from_utf8(output.stdout)?;
    let temporary = tempfile::tempdir()?;
    let checkout = temporary.path().join("checkout");
    checked(
        Command::new("git")
            .args(["clone", "--no-local", "--no-checkout", "--quiet"])
            .arg(&root)
            .arg(&checkout),
        "standalone Git clone",
    )?;
    checked(
        Command::new("git")
            .current_dir(&checkout)
            .args(["fetch", "--quiet", "--no-tags"])
            .arg(&root)
            .arg(commit.trim()),
        "fetch exact export commit",
    )?;
    git(
        &checkout,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "checkout",
            "--quiet",
            "--detach",
            commit.trim(),
        ],
        None,
    )?;
    let lock_name = if npm {
        "package-lock.json"
    } else {
        "Cargo.lock"
    };
    let seed = fs::read_to_string(checkout.join(lock_name))?;
    if npm {
        checked(
            Command::new("npm").current_dir(&checkout).args([
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--offline",
                "--no-audit",
                "--no-fund",
            ]),
            "npm lock resolution",
        )?;
    } else {
        let output = Command::new("cargo")
            .current_dir(&checkout)
            .args(["metadata", "--format-version", "1", "--all-features"])
            .output()?;
        ensure!(
            output.status.success(),
            "standalone Cargo resolution failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata: Value = serde_json::from_slice(&output.stdout)?;
        check_identities(&metadata, source)?;
    }
    let resolved = fs::read_to_string(checkout.join(lock_name))?;
    check_locked_versions(&seed, &resolved, npm)?;
    Ok(resolved)
}

fn checked(command: &mut Command, context: &str) -> Result<()> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "{context} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn check_identities(metadata: &Value, source: &str) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    let members = metadata["workspace_members"]
        .as_array()
        .context("workspace members")?;
    let expected = format!("git+https://github.com/neverhuman/jeryu.git?rev={source}#{source}");
    for package in metadata["packages"]
        .as_array()
        .context("resolved packages")?
    {
        let name = package["name"].as_str().context("package name")?;
        if !name.starts_with("jeryu-") {
            continue;
        }
        ensure!(
            names.insert(name),
            "duplicate Jeryu package identity: {name}"
        );
        if members.contains(&package["id"]) {
            ensure!(
                package["source"].is_null(),
                "component member is not local: {name}"
            );
        } else {
            ensure!(
                package["source"].as_str() == Some(expected.as_str()),
                "external Jeryu package does not bind source commit: {name}"
            );
        }
    }
    Ok(())
}

fn check_locked_versions(seed: &str, resolved: &str, npm: bool) -> Result<()> {
    if npm {
        let seed: Value = serde_json::from_str(seed)?;
        let resolved: Value = serde_json::from_str(resolved)?;
        for (path, package) in resolved["packages"].as_object().context("npm packages")? {
            if !path.contains("node_modules/") || package["link"].as_bool() == Some(true) {
                continue;
            }
            let original = seed["packages"]
                .get(path)
                .context("npm added an unlocked package")?;
            for field in ["version", "integrity", "resolved"] {
                ensure!(
                    original[field] == package[field],
                    "npm resolution changed locked {field} for {path}"
                );
            }
        }
    } else {
        let seed: toml::Value = toml::from_str(seed)?;
        let resolved: toml::Value = toml::from_str(resolved)?;
        for package in resolved["package"].as_array().context("Cargo packages")? {
            if package["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("jeryu-"))
            {
                continue;
            }
            ensure!(
                seed["package"]
                    .as_array()
                    .context("seed packages")?
                    .iter()
                    .any(|original| ["name", "version", "source", "checksum"]
                        .iter()
                        .all(|field| original.get(*field) == package.get(*field))),
                "Cargo resolution changed an external locked package: {}",
                package["name"]
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_projection_preserves_integrity_and_relocates_workspace_links() {
        let package = serde_json::json!({"name":"jeryu", "workspaces":["components/jeryu-web/apps/web"], "scripts":{"build":"node components/jeryu-web/build.mjs"}});
        let lock = serde_json::json!({"name":"jeryu", "packages":{"":{"name":"jeryu"}, "components/jeryu-web/apps/web":{"name":"@jeryu/web"}, "node_modules/@jeryu/web":{"link":true,"resolved":"components/jeryu-web/apps/web"}, "node_modules/example":{"version":"1.2.3","integrity":"sha512-fixture"}}});
        let (package, lock) = project_npm(package, lock, "components/jeryu-web").unwrap();
        assert_eq!(package["workspaces"][0], "apps/web");
        assert_eq!(package["scripts"]["build"], "node build.mjs");
        assert_eq!(
            lock["packages"]["node_modules/@jeryu/web"]["resolved"],
            "apps/web"
        );
        assert_eq!(
            lock["packages"]["node_modules/example"]["integrity"],
            "sha512-fixture"
        );
        assert!(
            lock["packages"]
                .get("components/jeryu-web/apps/web")
                .is_none()
        );
    }

    #[test]
    fn duplicate_local_and_git_identities_are_rejected() {
        let metadata = serde_json::json!({"workspace_members":["local"], "packages":[
            {"name":"jeryu-one", "id":"local", "source":null},
            {"name":"jeryu-one", "id":"remote", "source":"git+https://github.com/neverhuman/jeryu.git?rev=abc#abc"}
        ]});
        assert!(
            check_identities(&metadata, "abc")
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }

    #[test]
    fn lock_resolution_cannot_upgrade_dependencies() {
        let seed = "[[package]]\nname='external'\nversion='1.0.0'\nsource='registry+fixture'\nchecksum='abc'\n";
        assert!(check_locked_versions(seed, seed, false).is_ok());
        assert!(check_locked_versions(seed, &seed.replace("1.0.0", "1.0.1"), false).is_err());
    }
}
