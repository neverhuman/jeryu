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
