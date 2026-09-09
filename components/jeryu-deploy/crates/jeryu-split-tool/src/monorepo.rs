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

pub(super) fn export_tree(
    root: &Path,
    component: &str,
    source: &str,
    resolve_lock: bool,
) -> Result<()> {
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
    ensure!(
        git(root, &["cat-file", "-t", source], None)?.trim() == "commit",
        "source must be a commit"
    );
    let source_tree = git(root, &["rev-parse", &format!("{source}:{prefix}")], None)?;
    let npm = component == "jeryu-web";
    let mut files = BTreeMap::new();
    if npm {
        files = crate::split_export::npm_files(root, source, &prefix)?;
    } else {
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
            "component has no Rust members"
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
            if let Some(path) = dep.get("path").and_then(Value::as_str).and_then(|p| {
                if p == prefix {
                    Some(".")
                } else {
                    p.strip_prefix(&format!("{prefix}/"))
                }
            }) {
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
        if let Some(profile) = cargo.get("profile") {
            standalone
                .as_table_mut()
                .unwrap()
                .insert("profile".into(), profile.clone());
        }
        files.insert("Cargo.toml".into(), toml::to_string_pretty(&standalone)?);
        files.insert(
            "Cargo.lock".into(),
            git(root, &["show", &format!("{source}:Cargo.lock")], None)?,
        );
    }
    for path in ["LICENSE", "rust-toolchain.toml", ".cargo/config.toml"] {
        files.insert(
            path.into(),
            git(root, &["show", &format!("{source}:{path}")], None)?,
        );
    }
    files.insert("CONTRIBUTING.md".into(), format!(
        "# Contributing\n\nThis repository is a generated downstream mirror of [neverhuman/jeryu](https://github.com/neverhuman/jeryu). Submit changes there, under `{prefix}/`.\n\nThis preview binds source commit `{source}` in `.jeryu-source.json`. Generation and independent checks do not establish publication qualification.\n"));
    for path in ["AGENTS.md", "README.md"] {
        files.insert(
            format!("docs/split-original-guidance/{path}"),
            git(root, &["show", &format!("{source}:{prefix}/{path}")], None)?,
        );
    }
    files.insert("AGENTS.md".into(), format!(
        "# {component} mirror instructions\n\nThis is a generated downstream repository. Develop changes in [neverhuman/jeryu](https://github.com/neverhuman/jeryu), under `{prefix}/`. Do not run a competing source writer or edit generated manifests, locks, provenance, or workflow files here.\n\nUse `bash scripts/split-ci.sh` for independent verification. Original ownership and proof guidance is retained in [the imported instructions](docs/split-original-guidance/AGENTS.md); historical source routing and release authority declarations there are superseded by monorepo development. Preserve all immutable tags and use protected forward-only mirror updates after qualification.\n"));
    files.insert("README.md".into(), format!(
        "# {component}\n\nA generated component of [Jeryu](https://github.com/neverhuman/jeryu), licensed under Apache-2.0. This tree derives from monorepo commit `{source}`; `.jeryu-source.json` records its provenance and qualification status.\n\nRun `bash scripts/split-ci.sh` for independent checks with the pinned Rust toolchain, Node.js 26.1.0, Git, jq, ripgrep, a C compiler, pkg-config and OpenSSL development headers. Cross-component Rust dependencies resolve the originating public monorepo commit. Runner sandbox checks additionally require a disposable capable Linux host and `JERYU_DISPOSABLE_SANDBOX=1 bash scripts/split-ci.sh sandbox`.\n\nDevelop and review changes in the monorepo. See [contribution instructions](CONTRIBUTING.md) and [the imported component documentation](docs/split-original-guidance/README.md). Full forge installation and release qualification are maintained centrally.\n"));
    let temporary = tempfile::tempdir()?;
    let index = temporary.path().join("index");
    git(root, &["read-tree", source_tree.trim()], Some(&index))?;
    // Keep original workflow bytes as provenance without activating private writers.
    let paths = git(
        root,
        &["ls-tree", "-r", "--name-only", source_tree.trim()],
        None,
    )?;
    for path in paths
        .lines()
        .filter(|path| path.starts_with(".github/workflows/"))
    {
        let original = git(
            root,
            &[
                "show",
                &format!("{source_tree}:{path}", source_tree = source_tree.trim()),
            ],
            None,
        )?;
        put_file(
            root,
            &index,
            &format!(
                "docs/split-original-workflows/{}",
                path.strip_prefix(".github/workflows/").unwrap()
            ),
            original.as_bytes(),
        )?;
        git(
            root,
            &["update-index", "--force-remove", "--", path],
            Some(&index),
        )?;
    }
    files.extend(crate::split_export::ci_files(component));
    for (path, content) in files {
        put_file(root, &index, &path, content.as_bytes())?;
    }
    if resolve_lock {
        let tree = git(root, &["write-tree"], Some(&index))?;
        let lock = crate::split_export::resolve_lock(root, tree.trim(), source, npm)?;
        put_file(
            root,
            &index,
            if npm {
                "package-lock.json"
            } else {
                "Cargo.lock"
            },
            lock.as_bytes(),
        )?;
    }
    let provenance = serde_json::to_vec_pretty(&serde_json::json!({
        "schema_version": "jeryu.split-provenance/v1", "source_commit": source,
        "component": component, "original_component_tree": source_tree.trim(),
        "lock_regeneration_required": !resolve_lock, "publication_qualified": false,
    }))?;
    put_file(root, &index, ".jeryu-source.json", &provenance)?;
    let tree = git(root, &["write-tree"], Some(&index))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "component": component, "source_commit": source, "tree": tree.trim(),
            "lock_regeneration_required": !resolve_lock,
            "publication_qualified": false, "next": if resolve_lock { "prove independent build and CI, then publish through protected forward-only update" } else { "resolve standalone lock, prove independent build and CI, then publish through protected forward-only update" },
        }))?
    );
    Ok(())
}

fn put_file(root: &Path, index: &Path, path: &str, bytes: &[u8]) -> Result<()> {
    let blob = hash_blob(root, bytes)?;
    git(
        root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},{path}"),
        ],
        Some(index),
    )?;
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

pub(super) fn git(root: &Path, args: &[&str], index: Option<&Path>) -> Result<String> {
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
