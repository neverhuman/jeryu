//! Standalone dependency projections and lock validation for split previews.

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

use crate::split_tree::git;

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
            format!("{}\n", crate::canonical_json::pretty(package)?),
        ),
        (
            "package-lock.json".into(),
            format!("{}\n", crate::canonical_json::pretty(lock)?),
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

/// Resolve exact export locks, retaining scratch for supervised custody.
/// This returns a lock, never a publication or anonymous-build attestation.
pub(super) fn resolve_lock(
    root: &Path,
    tree: &str,
    source: &str,
    npm: bool,
    prepare_local: Option<&Path>,
    component: &str,
) -> Result<String> {
    let root = root.canonicalize()?;
    // Keeping the directory immediately prevents error paths from discarding
    // a failed clone or resolver output. The supervising caller owns retirement.
    let temporary = tempfile::Builder::new()
        .prefix("jeryu-split-lock.")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?
        .keep();
    eprintln!("retained split lock scratch: {}", temporary.display());
    let helper = temporary.join("source-build.sh");
    fs::write(
        &helper,
        git(
            &root,
            &["show", &format!("{source}:scripts/source-build.sh")],
            None,
        )?,
    )?;
    let component_tree = git(
        &root,
        &["rev-parse", &format!("{source}:components/{component}")],
        None,
    )?;
    let before = prepare_local
        .map(|local| local_snapshot(&helper, local, source, component, component_tree.trim()))
        .transpose()
        .with_context(|| {
            format!(
                "local source refused; retained resolver scratch {}",
                temporary.display()
            )
        })?;
    if let (Some(local), Some(snapshot)) = (prepare_local, before.as_ref()) {
        fs::write(
            temporary.join("local-source.path"),
            local.as_os_str().as_encoded_bytes(),
        )?;
        fs::write(temporary.join("local-source.before"), snapshot)?;
        fs::write(
            temporary.join("transport.txt"),
            "local-source-preparation; public origin unproven\n",
        )?;
    }
    let result = (|| {
        let output = crate::split_tree::source_git_command(&root)
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
        let checkout = temporary.join("checkout");
        checked(
            crate::split_tree::source_git_command(&root)
                .args(["clone", "--no-local", "--no-checkout", "--quiet"])
                .arg(&root)
                .arg(&checkout),
            "standalone Git clone",
        )?;
        checked(
            crate::split_tree::source_git_command(&root)
                .current_dir(&checkout)
                .args(["fetch", "--quiet", "--no-tags"])
                .arg(&root)
                .arg(commit.trim()),
            "fetch exact export commit",
        )?;
        checked(
            crate::split_tree::source_git_command(&root)
                .current_dir(&checkout)
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "checkout",
                    "--quiet",
                    "--detach",
                    commit.trim(),
                ]),
            "checkout exact export commit",
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
            let mut command = if let Some(local) = prepare_local {
                local_cargo(&helper, local, source, component, component_tree.trim())
            } else {
                let mut command = Command::new("cargo");
                command.args(["metadata", "--format-version", "1", "--all-features"]);
                command
            };
            let output = command.current_dir(&checkout).output()?;
            fs::write(temporary.join("metadata.stdout"), &output.stdout)?;
            fs::write(temporary.join("metadata.stderr"), &output.stderr)?;
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
        // npm may reorder equivalent objects; retain the deterministic seed.
        Ok(if npm { seed } else { resolved })
    })();
    if let Some(local) = prepare_local {
        let after = local_snapshot(&helper, local, source, component, component_tree.trim())?;
        fs::write(temporary.join("local-source.after"), &after)?;
        ensure!(
            Some(after) == before,
            "local split source changed during resolution"
        );
    }
    result
}

fn local_snapshot(
    helper: &Path,
    local: &Path,
    source: &str,
    component: &str,
    tree: &str,
) -> Result<Vec<u8>> {
    let output = Command::new("/bin/bash")
        .env_remove("BASH_ENV")
        .env_remove("ENV")
        .args([
            "-c",
            "source \"$1\"; split_source_snapshot \"$2\" \"$3\" \"$4\" \"$5\"",
            "split-source",
        ])
        .arg(helper)
        .arg(local)
        .args([source, component, tree])
        .output()?;
    ensure!(
        output.status.success() && !output.stdout.is_empty(),
        "local split source admission failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn local_cargo(helper: &Path, local: &Path, source: &str, component: &str, tree: &str) -> Command {
    let mut command = Command::new("/bin/bash");
    command.env_remove("BASH_ENV").env_remove("ENV")
        .args(["-c", "source \"$1\"; split_source_run \"$2\" \"$3\" \"$4\" \"$5\" cargo metadata --format-version 1 --all-features", "split-source"])
        .arg(helper).arg(local).args([source, component, tree]);
    command
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
        ensure!(
            seed == resolved,
            "npm resolution changed projected lock metadata"
        );
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
        let npm = r#"{"packages":{"node_modules/example":{"version":"1.0.0","integrity":"sha512-fixture"}}}"#;
        assert!(check_locked_versions(npm, npm, true).is_ok());
        assert!(
            check_locked_versions(npm, &npm.replace("sha512-fixture", "sha512-tampered"), true)
                .is_err()
        );
    }
    #[test]
    fn local_transport_keeps_public_source_identity_checks() {
        let source = "a".repeat(40);
        let public = format!("git+https://github.com/neverhuman/jeryu.git?rev={source}#{source}");
        let mut metadata = serde_json::json!({"workspace_members":["local"], "packages":[
            {"name":"jeryu-one", "id":"local", "source":null},
            {"name":"jeryu-two", "id":"remote", "source":public}
        ]});
        assert!(check_identities(&metadata, &source).is_ok());
        for invalid in [
            "git+file:///fixture/source#revision",
            "git+https://github.com/neverhuman/jeryu.git?rev=other#other",
        ] {
            metadata["packages"][1]["source"] = Value::String(invalid.into());
            assert!(check_identities(&metadata, &source).is_err());
        }
    }

    #[test]
    fn local_cargo_passes_source_as_arguments_without_changing_metadata_request() {
        let command = local_cargo(
            Path::new("/fixture/helper"),
            Path::new("/fixture/source"),
            &"a".repeat(40),
            "jeryu-cache",
            &"b".repeat(40),
        );
        assert_eq!(command.get_program(), "/bin/bash");
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_str().unwrap())
            .collect();
        assert_eq!(args[0], "-c");
        assert!(args[1].ends_with("cargo metadata --format-version 1 --all-features"));
        assert_eq!(
            args[2..],
            [
                "split-source",
                "/fixture/helper",
                "/fixture/source",
                &"a".repeat(40),
                "jeryu-cache",
                &"b".repeat(40)
            ]
        );
        assert!(!args[1].contains("file://"));
    }
    #[test]
    fn refused_local_admission_retains_the_exact_resolver_scratch() {
        use std::os::unix::fs::MetadataExt;
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::create_dir_all(root.join("components/jeryu-cache")).unwrap();
        let helper = "split_source_snapshot() { return 79; }\n";
        fs::write(root.join("scripts/source-build.sh"), helper).unwrap();
        fs::write(root.join("components/jeryu-cache/Cargo.toml"), "fixture\n").unwrap();
        for args in [
            vec!["init", "--quiet", "--initial-branch=main", "--template="],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@jeryu.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "Synthetic source admission",
            ],
        ] {
            let output = crate::split_tree::source_git_command(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let source = git(root, &["rev-parse", "HEAD"], None).unwrap();
        let tree = git(root, &["rev-parse", "HEAD^{tree}"], None).unwrap();
        let error = resolve_lock(
            root,
            tree.trim(),
            source.trim(),
            false,
            Some(root),
            "jeryu-cache",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("local split source admission failed"));
        let message = error.to_string();
        let retained = Path::new(
            message
                .strip_prefix("local source refused; retained resolver scratch ")
                .unwrap(),
        );
        let retained_metadata = fs::symlink_metadata(retained).unwrap();
        assert!(retained_metadata.file_type().is_dir());
        assert_eq!(retained_metadata.permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::read_dir(retained).unwrap().count(), 1);
        let input = retained.join("source-build.sh");
        let metadata = fs::symlink_metadata(&input).unwrap();
        assert!(metadata.file_type().is_file());
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(fs::read_to_string(&input).unwrap(), helper);
        assert!(!retained.join("checkout").exists());
        // Only this known fixture file/directory can be unlinked; never recurse
        // through a resolver attempt whose contents are not the asserted input.
        fs::remove_file(input).unwrap();
        fs::remove_dir(retained).unwrap();
    }
}
