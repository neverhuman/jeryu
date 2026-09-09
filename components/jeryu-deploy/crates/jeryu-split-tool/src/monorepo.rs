//! Monorepo manifest, package identity and public dependency checks.

use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};
use toml::Value;

pub(super) fn validate_manifest(manifest: &Value, check_paths: bool) -> Result<()> {
    validate_storage(manifest)?;
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

fn validate_storage(manifest: &Value) -> Result<()> {
    let storage = manifest
        .get("storage")
        .context("storage policy is missing")?;
    ensure!(
        storage.get("default_backend").and_then(Value::as_str) == Some("sqlite")
            && storage.get("bundled_sqlite").and_then(Value::as_bool) == Some(true),
        "standalone releases require bundled SQLite by default"
    );
    let redline = manifest
        .get("redline")
        .context("Redline proof policy is missing")?;
    ensure!(
        redline.get("role").and_then(Value::as_str) == Some("optional-compatibility-proof")
            && redline.get("required_for_release").and_then(Value::as_bool) == Some(false)
            && redline.get("contract_manifest").and_then(Value::as_str)
                == Some("components/jeryu-release-ops/tests/redline/Cargo.toml"),
        "Redline compatibility must remain separate from the SQLite release"
    );
    ensure!(
        redline
            .get("two_consumer_proof_required")
            .and_then(Value::as_bool)
            == Some(true),
        "optional Redline qualification still requires its two-consumer proof"
    );
    Ok(())
}

fn validate_sqlite_lock(lock: &Value) -> Result<()> {
    for package in lock["package"].as_array().context("lock packages")? {
        let name = package["name"].as_str().context("locked package name")?;
        let source = package.get("source").and_then(Value::as_str).unwrap_or("");
        ensure!(
            !name.starts_with("redlinedb") && !source.contains("/redline-core"),
            "SQLite release graph must not resolve Redline: {name}"
        );
    }
    Ok(())
}

pub(super) fn check(root: &Path) -> Result<()> {
    let lock: Value = toml::from_str(&fs::read_to_string(root.join("Cargo.lock"))?)?;
    validate_sqlite_lock(&lock)?;
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

fn public_auditor_repository(tool: &Value) -> Result<&str> {
    ensure!(
        tool.get("schema_version").and_then(Value::as_str) == Some("2"),
        "public auditor distribution requires tool-manifest schema_version \"2\""
    );
    ensure!(
        tool.get("jankurai")
            .and_then(|pin| pin.get("repo"))
            .and_then(Value::as_str)
            == Some("http://127.0.0.1:8787/git/jeryu/jankurai.git"),
        "governed auditor producer identity changed"
    );
    let distribution = tool
        .get("distribution")
        .and_then(Value::as_table)
        .context("public auditor distribution is missing")?;
    ensure!(
        distribution.len() == 1 && distribution.contains_key("source_repository"),
        "public auditor distribution must contain only source_repository"
    );
    let repository = distribution
        .get("source_repository")
        .and_then(Value::as_str)
        .context("public auditor source_repository must be a string")?;
    ensure!(
        repository == "https://github.com/neverhuman/jankurai.git",
        "public auditor transport is not the approved Jankurai mirror"
    );
    Ok(repository)
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
    match public_auditor_repository(&tool) {
        Ok(repository) => {
            println!(
                "governed auditor producer: {}",
                tool["jankurai"]["repo"]
                    .as_str()
                    .context("auditor producer")?
            );
            println!("governed auditor public transport: {repository}");
            refs.insert((
                repository.to_owned(),
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
        Err(error) => {
            failures.insert(error.to_string());
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn auditor_manifest() -> Value {
        toml::from_str(
            r#"
schema_version = "2"
[jankurai]
repo = "http://127.0.0.1:8787/git/jeryu/jankurai.git"
[distribution]
source_repository = "https://github.com/neverhuman/jankurai.git"
"#,
        )
        .expect("auditor manifest")
    }

    #[test]
    fn sqlite_policy_rejects_missing_malformed_or_blocking_redline_settings() {
        let manifest: Value = toml::from_str(
            r#"
[storage]
default_backend = "sqlite"
bundled_sqlite = true
[redline]
role = "optional-compatibility-proof"
required_for_release = false
contract_manifest = "components/jeryu-release-ops/tests/redline/Cargo.toml"
two_consumer_proof_required = true
"#,
        )
        .unwrap();
        validate_storage(&manifest).unwrap();
        for table in ["storage", "redline"] {
            let mut missing = manifest.clone();
            missing.as_table_mut().unwrap().remove(table);
            assert!(validate_storage(&missing).is_err());
            for key in if table == "storage" {
                vec!["default_backend", "bundled_sqlite"]
            } else {
                vec![
                    "role",
                    "required_for_release",
                    "contract_manifest",
                    "two_consumer_proof_required",
                ]
            } {
                for invalid in [
                    None,
                    Some(Value::Integer(0)),
                    Some(Value::String("invalid".into())),
                ] {
                    let mut changed = manifest.clone();
                    let settings = changed[table].as_table_mut().unwrap();
                    settings.remove(key);
                    if let Some(value) = invalid {
                        settings.insert(key.into(), value);
                    }
                    assert!(validate_storage(&changed).is_err(), "{table}.{key}");
                }
            }
        }
        let mut required = manifest.clone();
        required["redline"]["required_for_release"] = Value::Boolean(true);
        assert!(validate_storage(&required).is_err());
        let mut waived = manifest.clone();
        waived["redline"]["two_consumer_proof_required"] = Value::Boolean(false);
        assert!(validate_storage(&waived).is_err());
    }

    #[test]
    fn sqlite_lock_rejects_redline_packages_and_renamed_git_edges() {
        let lock: Value =
            toml::from_str("[[package]]\nname='rusqlite'\nversion='0.32.1'\n").unwrap();
        validate_sqlite_lock(&lock).unwrap();
        for name in [
            "redlinedb",
            "redlinedb-core",
            "redlinedb-parser",
            "redlinedb-io",
        ] {
            let mut changed = lock.clone();
            changed["package"][0]["name"] = Value::String(name.into());
            assert!(validate_sqlite_lock(&changed).is_err());
        }
        let mut alias = lock;
        alias["package"][0].as_table_mut().unwrap().insert(
            "source".into(),
            Value::String(
                "git+https://github.com/neverhuman/redline-core.git?tag=example#abc".into(),
            ),
        );
        assert!(validate_sqlite_lock(&alias).is_err());
    }

    #[test]
    fn public_auditor_transport_keeps_the_original_producer_identity() {
        let manifest = auditor_manifest();
        assert_eq!(
            public_auditor_repository(&manifest).expect("public mirror"),
            "https://github.com/neverhuman/jankurai.git"
        );
        assert_eq!(
            manifest["jankurai"]["repo"].as_str(),
            Some("http://127.0.0.1:8787/git/jeryu/jankurai.git")
        );
    }

    #[test]
    fn public_auditor_transport_rejects_legacy_missing_and_extra_distribution() {
        let mut legacy = auditor_manifest();
        legacy["schema_version"] = Value::String("1".to_owned());
        legacy
            .as_table_mut()
            .expect("manifest")
            .remove("distribution");
        assert!(public_auditor_repository(&legacy).is_err());

        let mut missing = auditor_manifest();
        missing
            .as_table_mut()
            .expect("manifest")
            .remove("distribution");
        assert!(public_auditor_repository(&missing).is_err());

        let mut extra = auditor_manifest();
        extra["distribution"]
            .as_table_mut()
            .expect("distribution")
            .insert("fallback".to_owned(), Value::Boolean(true));
        assert!(public_auditor_repository(&extra).is_err());
    }

    #[test]
    fn public_auditor_transport_rejects_unapproved_urls_and_producer_changes() {
        for repository in [
            "http://github.com/neverhuman/jankurai.git",
            "https://github.com.evil.invalid/neverhuman/jankurai.git",
            "https://user@github.com/neverhuman/jankurai.git",
            "https://github.com/neverhuman/jankurai.git?ref=main",
            "https://github.com/neverhuman/jankurai.git#main",
            "https://github.com/neverhuman/jankurai.git/",
            "https://github.com/neverhuman/another-tool.git",
        ] {
            let mut manifest = auditor_manifest();
            manifest["distribution"]["source_repository"] = Value::String(repository.to_owned());
            assert!(public_auditor_repository(&manifest).is_err());
        }
        let mut changed = auditor_manifest();
        changed["jankurai"]["repo"] =
            Value::String("https://github.com/neverhuman/jankurai.git".to_owned());
        assert!(public_auditor_repository(&changed).is_err());
    }
}
