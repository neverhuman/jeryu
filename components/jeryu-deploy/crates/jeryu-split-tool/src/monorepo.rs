//! Monorepo identity checks and deterministic split-tree preparation.

use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use toml::Value;

pub(super) fn validate_manifest(manifest: &Value, check_paths: bool) -> Result<()> {
    ensure!(
        manifest["repo_family"].as_str() == Some("jeryu-split"),
        "family identity changed"
    );
    ensure!(
        manifest["release_lineage"].as_str() == Some("v5"),
        "release lineage changed"
    );
    ensure!(
        manifest["status"].as_str() == Some("candidate")
            && manifest["formal_ga"].as_bool() == Some(false),
        "candidate metadata must remain fail-closed"
    );
    ensure!(
        manifest["handover"]["status"].as_str() == Some("pending-protected-review"),
        "authority handover requires separate protected implementation and evidence"
    );
    let repos = manifest["repo"]
        .as_array()
        .context("repository inventory")?;
    ensure!(
        repos.len() == 11,
        "original repository disposition is incomplete"
    );
    let mut names = BTreeSet::new();
    for repo in repos {
        let name = repo["name"].as_str().context("repository name")?;
        ensure!(names.insert(name), "duplicate repository {name}");
        let path = repo["path"].as_str().context("component path")?;
        let expected = if name == "jeryu" {
            ".".into()
        } else {
            format!("components/{name}")
        };
        ensure!(path == expected, "noncanonical component path for {name}");
        ensure!(
            repo["mirror_github_main"].as_bool() == Some(false),
            "competing hosted-to-GitHub writer declared"
        );
        if check_paths {
            ensure!(Path::new(path).is_dir(), "component is missing: {name}");
        }
    }
    Ok(())
}

pub(super) fn check(root: &Path) -> Result<()> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()?;
    ensure!(output.status.success(), "locked Cargo metadata failed");
    let data: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages = data["packages"].as_array().context("Cargo packages")?;
    ensure!(
        packages.len() == 65,
        "expected all 65 original Rust packages"
    );
    let mut names = BTreeSet::new();
    for package in packages {
        let name = package["name"].as_str().context("package name")?;
        ensure!(names.insert(name), "duplicate package {name}");
        ensure!(package["source"].is_null(), "nonlocal Jeryu package {name}");
        ensure!(
            matches!(
                package["license"].as_str(),
                Some("Apache-2.0" | "MIT OR Apache-2.0")
            ),
            "package {name} does not retain its Apache-2.0 licensing option"
        );
        for dependency in package["dependencies"].as_array().context("dependencies")? {
            if dependency["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("jeryu-"))
            {
                ensure!(
                    dependency["source"].is_null(),
                    "Jeryu dependency escaped the workspace"
                );
            }
        }
    }
    println!("65 packages; unique local Jeryu identities; Apache-2.0 licensing options preserved");
    Ok(())
}

pub(super) fn public_preflight(root: &Path) -> Result<()> {
    let lock: Value = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    let mut failures = BTreeSet::new();
    let mut refs = BTreeSet::new();
    for package in lock["package"].as_array().context("lock packages")? {
        let Some(source) = package.get("source").and_then(Value::as_str) else {
            continue;
        };
        if source.starts_with("git+")
            && (!source.starts_with("git+https://github.com/neverhuman/") || !source.contains('#'))
        {
            failures.insert(format!(
                "{} uses a nonpublic Git source",
                package["name"].as_str().unwrap_or("dependency")
            ));
        } else if let Some(source) = source.strip_prefix("git+") {
            if let Some((coordinate, sha)) = source.split_once('#')
                && let Some((remote, tag)) = coordinate.split_once("?tag=")
            {
                refs.insert((remote.to_owned(), tag.to_owned(), sha.to_owned()));
            } else {
                failures.insert("external Git dependency must bind an immutable public tag".into());
            }
        }
    }
    let tool: Value = toml::from_str(&fs::read_to_string(
        root.join("components/jeryu-tool/tool-manifest.toml"),
    )?)?;
    if !tool["jankurai"]["repo"]
        .as_str()
        .is_some_and(|url| url.starts_with("https://github.com/neverhuman/"))
    {
        failures.insert("governed auditor source is not public".into());
    } else {
        refs.insert((
            tool["jankurai"]["repo"]
                .as_str()
                .context("auditor remote")?
                .to_owned(),
            tool["jankurai"]["tag"]
                .as_str()
                .context("auditor tag")?
                .to_owned(),
            tool["jankurai"]["rev"]
                .as_str()
                .context("auditor commit")?
                .to_owned(),
        ));
    }
    for (remote, tag, expected) in refs {
        // Empty credentials/configuration are essential: a personal rewrite or
        // cached source must not make an unpublished dependency look public.
        let output = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args([
                "-c",
                "credential.helper=",
                "-c",
                "http.followRedirects=false",
                "-c",
                "http.lowSpeedLimit=1",
                "-c",
                "http.lowSpeedTime=20",
                "ls-remote",
                "--exit-code",
                "--tags",
                &remote,
                &format!("refs/tags/{tag}"),
                &format!("refs/tags/{tag}^{{}}"),
            ])
            .output()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let advertised: BTreeMap<_, _> = text
            .lines()
            .filter_map(|line| line.split_once('\t').map(|(sha, name)| (name, sha)))
            .collect();
        let peeled = format!("refs/tags/{tag}^{{}}");
        let direct = format!("refs/tags/{tag}");
        let actual = advertised
            .get(peeled.as_str())
            .or_else(|| advertised.get(direct.as_str()));
        if !output.status.success() || actual.copied() != Some(expected.as_str()) {
            failures.insert(format!(
                "{remote} does not anonymously advertise {tag} at {expected}"
            ));
        }
    }
    ensure!(
        failures.is_empty(),
        "public preflight failed:\n{}",
        failures.into_iter().collect::<Vec<_>>().join("\n")
    );
    println!(
        "public immutable tags match anonymously; a fresh clone/build remains a separate required proof"
    );
    Ok(())
}

pub(super) fn export_tree(root: &Path, component: &str, source: &str) -> Result<()> {
    ensure!(
        source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit()),
        "--source requires a full commit SHA"
    );
    ensure!(
        component.starts_with("jeryu-")
            && component
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-'),
        "invalid component name"
    );
    let prefix = format!("components/{component}");
    let source_tree = git(root, &["rev-parse", &format!("{source}:{prefix}")], None)?;
    let mut standalone: Value = if let Ok(manifest) = git(
        root,
        &["show", &format!("{source}:{prefix}/Cargo.toml")],
        None,
    ) {
        toml::from_str(&manifest)?
    } else {
        Value::Table(toml::Table::new())
    };
    let cargo: Value = toml::from_str(&git(
        root,
        &["show", &format!("{source}:Cargo.toml")],
        None,
    )?)?;
    let workspace = standalone_workspace(&cargo, &prefix, source)?;
    ensure!(
        !workspace["members"].as_array().unwrap().is_empty(),
        "component has no Rust members; npm export is not implemented"
    );
    standalone
        .as_table_mut()
        .context("standalone manifest")?
        .insert("workspace".into(), workspace);
    let mut patches = toml::Table::new();
    for (name, dep) in cargo["workspace"]["dependencies"]
        .as_table()
        .context("workspace dependencies")?
    {
        if let Some(path) = dep
            .get("path")
            .and_then(Value::as_str)
            .and_then(|p| p.strip_prefix(&format!("{prefix}/")))
        {
            patches.insert(
                name.clone(),
                Value::Table(toml::Table::from_iter([(
                    "path".into(),
                    Value::String(path.into()),
                )])),
            );
        }
    }
    standalone.as_table_mut().unwrap().insert(
        "patch".into(),
        Value::Table(toml::Table::from_iter([(
            "https://github.com/neverhuman/jeryu.git".into(),
            Value::Table(patches),
        )])),
    );
    let temporary = tempfile::tempdir()?;
    let index = temporary.path().join("index");
    git(root, &["read-tree", source_tree.trim()], Some(&index))?;
    let manifest = toml::to_string_pretty(&standalone)?;
    let blob = hash_blob(root, manifest.as_bytes())?;
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},Cargo.toml"),
        ],
        Some(&index),
    )?;
    let provenance = serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version": "jeryu.split-provenance/v1", "source_commit": source,
        "component": component, "original_component_tree": source_tree.trim(),
        "lock_regeneration_required": true, "publication_qualified": false,
    }))?;
    let blob = hash_blob(root, &provenance)?;
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},.jeryu-source.json"),
        ],
        Some(&index),
    )?;
    let tree = git(root, &["write-tree"], Some(&index))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "component": component, "source_commit": source, "tree": tree.trim(),
            "publication_qualified": false, "next": "regenerate lock, prove independent build and CI, then publish through protected forward-only update",
        }))?
    );
    Ok(())
}

fn standalone_workspace(cargo: &Value, prefix: &str, source: &str) -> Result<Value> {
    let mut workspace = cargo["workspace"].clone();
    let prefix_slash = format!("{prefix}/");
    let relative = |path: &str| {
        if path == prefix {
            Some(".".to_owned())
        } else {
            path.strip_prefix(&prefix_slash).map(str::to_owned)
        }
    };
    for field in ["members", "default-members", "exclude"] {
        if let Some(array) = workspace.get(field).and_then(Value::as_array) {
            let values: Vec<_> = array
                .iter()
                .filter_map(Value::as_str)
                .filter_map(relative)
                .map(Value::String)
                .collect();
            if field == "default-members" && values.is_empty() {
                workspace.as_table_mut().unwrap().remove(field);
            } else {
                workspace[field] = Value::Array(values);
            }
        }
    }
    for (_, dep) in workspace["dependencies"]
        .as_table_mut()
        .context("workspace dependencies")?
        .iter_mut()
    {
        let Some(path) = dep.get("path").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        if let Some(path) = relative(&path) {
            dep["path"] = Value::String(path);
        } else {
            dep.as_table_mut()
                .context("path dependency")?
                .remove("path");
            dep.as_table_mut().unwrap().insert(
                "git".into(),
                Value::String("https://github.com/neverhuman/jeryu.git".into()),
            );
            dep.as_table_mut()
                .unwrap()
                .insert("rev".into(), Value::String(source.into()));
        }
    }
    Ok(workspace)
}

fn git(root: &Path, args: &[&str], index: Option<&Path>) -> Result<String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output()?;
    if !output.status.success() {
        bail!("Git export operation failed ({})", args[0]);
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn hash_blob(root: &Path, bytes: &[u8]) -> Result<String> {
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["hash-object", "-w", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().context("Git stdin")?.write_all(bytes)?;
    let output = child.wait_with_output()?;
    ensure!(output.status.success(), "cannot write generated Git blob");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_preserves_local_identity_and_pins_external_packages_to_source() {
        let cargo: Value = toml::from_str(
            r#"
            [workspace]
            members = ["components/jeryu-one/crates/one", "components/jeryu-two"]
            default-members = ["components/jeryu-two"]
            [workspace.dependencies]
            one = { path = "components/jeryu-one/crates/one" }
            two = { path = "components/jeryu-two" }
        "#,
        )
        .unwrap();
        let result = standalone_workspace(&cargo, "components/jeryu-one", &"a".repeat(40)).unwrap();
        assert_eq!(
            result["members"].as_array().unwrap(),
            &vec![Value::String("crates/one".into())]
        );
        assert!(result.get("default-members").is_none());
        assert_eq!(
            result["dependencies"]["one"]["path"].as_str(),
            Some("crates/one")
        );
        assert_eq!(
            result["dependencies"]["two"]["git"].as_str(),
            Some("https://github.com/neverhuman/jeryu.git")
        );
        assert_eq!(
            result["dependencies"]["two"]["rev"].as_str(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
        assert!(result["dependencies"]["two"].get("path").is_none());
    }
}
