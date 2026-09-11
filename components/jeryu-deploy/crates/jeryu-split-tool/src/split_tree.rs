//! Deterministic Git tree assembly for standalone component exports.

use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use toml::Value;

pub(super) fn export_tree(
    root: &Path,
    component: &str,
    source: &str,
    resolve_lock: bool,
    prepare_local: Option<&Path>,
) -> Result<()> {
    ensure!(
        prepare_local.is_none() || resolve_lock,
        "--prepare-local requires --resolve-lock"
    );
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
    for path in [
        "LICENSE",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "scripts/source-build.sh",
    ] {
        files.insert(
            path.into(),
            git(root, &["show", &format!("{source}:{path}")], None)?,
        );
    }
    // Shared score transport is source-owned, not compiled into the exporter.
    // Generated mirrors receive the same source/directory custody helpers.
    let score_transport = matches!(
        component,
        "jeryu-core"
            | "jeryu-deploy"
            | "jeryu-jira"
            | "jeryu-intelligence"
            | "jeryu-release-ops"
            | "jeryu-tool"
            | "jeryu-web"
    );
    if score_transport {
        for path in [
            "scripts/check-audit-score.sh",
            "scripts/source-build.sh",
            "tests/scratch.sh",
        ] {
            files.insert(
                path.into(),
                git(root, &["show", &format!("{source}:{path}")], None)?,
            );
        }
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
    let score_guidance = if score_transport {
        "\n\nThe retained component score command requires its existing auditor setup. Its Rust report validator additionally needs rustup, lsof and GNU timeout on Linux. Run `bash ops/ci/score.sh` to build the validator from this descriptor's exact public monorepo commit, or explicitly use `bash ops/ci/score.sh --prepare-local /absolute/clean/monorepo` for preparatory local Git transport before that commit is public. Preparatory execution is not anonymous qualification. Failed or uncertain source-build attempts are retained for supervised custody."
    } else {
        ""
    };
    files.insert("README.md".into(), format!(
        "# {component}\n\nA generated component of [Jeryu](https://github.com/neverhuman/jeryu), licensed under Apache-2.0. This tree derives from monorepo commit `{source}`; `.jeryu-source.json` records its provenance and qualification status.\n\nRun `bash scripts/split-ci.sh` for independent checks with the pinned Rust toolchain, Node.js 26.1.0, Git, jq, ripgrep, a C compiler, pkg-config and OpenSSL development headers. Cross-component Rust dependencies resolve the originating public monorepo commit. Runner sandbox checks additionally require a disposable capable Linux host and `JERYU_DISPOSABLE_SANDBOX=1 bash scripts/split-ci.sh sandbox`.\n\nDevelop and review changes in the monorepo. See [contribution instructions](CONTRIBUTING.md) and [the imported component documentation](docs/split-original-guidance/README.md). Full forge installation and release qualification are maintained centrally. Before the source commit is public, `bash scripts/split-ci.sh ordinary --prepare-local /absolute/clean/monorepo` uses explicitly verified local transport while preserving public dependency identities. It is preparatory evidence, not anonymous qualification. Resolver scratch is retained for supervised custody.{score_guidance}\n"));
    let scratch = tempfile::tempdir()?;
    let index = scratch.path().join("index");
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
        let lock = crate::split_export::resolve_lock(
            root,
            tree.trim(),
            source,
            npm,
            prepare_local,
            component,
        )?;
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
    let provenance = crate::canonical_json::pretty(serde_json::json!({
        "schema_version": "jeryu.split-provenance/v1", "source_commit": source,
        "component": component, "original_component_tree": source_tree.trim(),
        "lock_regeneration_required": !resolve_lock, "publication_qualified": false,
    }))?;
    put_file(root, &index, ".jeryu-source.json", provenance.as_bytes())?;
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
    let mut command = source_git_command(root);
    command.args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output()?;
    if !output.status.success() {
        bail!("Git export operation failed ({})", args[0]);
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) fn source_git_command(root: &Path) -> Command {
    let mut command = Command::new("/usr/bin/git");
    command
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "core.fsmonitor=false"]);
    command
}

fn hash_blob(root: &Path, bytes: &[u8]) -> Result<String> {
    let mut child = source_git_command(root)
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
