//! Offline commit preparation. This module never fetches, pushes or updates refs.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct Request<'a> {
    pub source_repo: &'a Path,
    pub source: &'a str,
    pub component: &'a str,
    pub export_tree: &'a str,
    pub mirror: &'a Path,
    pub expected_tip: &'a str,
    pub expected_tags: &'a Path,
    pub initial_tip: Option<&'a str>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Provenance {
    schema_version: String,
    source_commit: String,
    component: String,
    original_component_tree: String,
    lock_regeneration_required: bool,
    publication_qualified: bool,
}

fn command(repo: &Path) -> Command {
    let mut command = crate::split_tree::source_git_command(repo);
    command.env("GIT_NO_LAZY_FETCH", "1").args([
        "-c",
        "protocol.allow=never",
        "-c",
        "protocol.file.allow=never",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "i18n.commitEncoding=UTF-8",
        "-c",
        "log.showSignature=false",
    ]);
    command
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = command(repo).args(args).output()?;
    ensure!(
        output.status.success() && output.stderr.is_empty(),
        "offline mirror Git operation failed: {}",
        args[0]
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn oid(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "expected lowercase full SHA-1 object ID"
    );
    Ok(())
}

fn physical(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute() && path.canonicalize()? == path,
        "repository path must be physical and absolute"
    );
    ensure!(
        fs::symlink_metadata(path)?.is_dir(),
        "repository is not a directory"
    );
    Ok(path.to_owned())
}

fn admit_repository(path: &Path, bare: bool) -> Result<PathBuf> {
    let path = physical(path)?;
    ensure!(
        git(&path, &["rev-parse", "--is-bare-repository"])?.trim()
            == if bare { "true" } else { "false" },
        "wrong Git repository kind"
    );
    ensure!(
        git(&path, &["rev-parse", "--show-object-format"])?.trim() == "sha1",
        "only existing SHA-1 mirrors are supported"
    );
    let storage = if bare {
        path.clone()
    } else {
        path.join(".git")
    };
    physical(&storage)?;
    ensure!(
        Path::new(git(&path, &["rev-parse", "--absolute-git-dir"])?.trim()) == storage,
        "unexpected Git directory"
    );
    for relative in [
        "shallow",
        "info/grafts",
        "objects/info/alternates",
        "commondir",
    ] {
        match fs::symlink_metadata(storage.join(relative)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => anyhow::bail!("external or shallow Git storage is unsupported: {relative}"),
        }
    }
    ensure!(
        git(
            &path,
            &["for-each-ref", "--format=%(refname)", "refs/replace/"]
        )?
        .is_empty(),
        "replacement refs are unsupported"
    );
    Ok(path)
}

fn refs(repo: &Path, prefix: &str) -> Result<String> {
    git(
        repo,
        &[
            "for-each-ref",
            "--sort=refname",
            "--format=%(objectname) %(refname)",
            prefix,
        ],
    )
}

fn clean_source(repo: &Path, source: &str) -> Result<()> {
    ensure!(
        git(repo, &["rev-parse", "HEAD"])?.trim() == source,
        "source HEAD differs from requested commit"
    );
    ensure!(
        git(repo, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty(),
        "source checkout is not clean"
    );
    ensure!(
        git(repo, &["ls-files", "-v"])?.lines().all(|line| line
            .as_bytes()
            .first()
            .is_some_and(|b| *b != b'S' && !b.is_ascii_lowercase())),
        "hidden index flags are unsupported"
    );
    Ok(())
}

fn descriptor(repo: &Path, treeish: &str) -> Result<Option<Provenance>> {
    let entry = git(repo, &["ls-tree", treeish, "--", ".jeryu-source.json"])?;
    if entry.is_empty() {
        return Ok(None);
    }
    ensure!(
        entry.lines().count() == 1
            && entry.starts_with("100644 blob ")
            && entry.ends_with("\t.jeryu-source.json\n"),
        "provenance must be one regular blob"
    );
    let bytes = git(repo, &["show", &format!("{treeish}:.jeryu-source.json")])?;
    Ok(Some(
        serde_json::from_str(&bytes).context("invalid split provenance")?,
    ))
}

fn validate_provenance(repo: &Path, value: &Provenance, component: &str) -> Result<()> {
    oid(&value.source_commit)?;
    oid(&value.original_component_tree)?;
    ensure!(
        value.schema_version == "jeryu.split-provenance/v1"
            && value.component == component
            && !value.lock_regeneration_required
            && !value.publication_qualified,
        "inconsistent or unresolved split provenance"
    );
    ensure!(
        git(repo, &["cat-file", "-t", &value.source_commit])?.trim() == "commit",
        "source provenance is not a commit"
    );
    ensure!(
        git(
            repo,
            &[
                "rev-parse",
                &format!("{}:components/{component}", value.source_commit)
            ]
        )?
        .trim()
            == value.original_component_tree,
        "component tree differs from originating source"
    );
    Ok(())
}

fn generator_binding(repo: &Path, source: &str) -> Result<BTreeMap<String, String>> {
    let mut blobs = BTreeMap::new();
    // Bind the implementation/templates actually compiled into this command.
    for (name, compiled) in [
        ("main.rs", include_str!("main.rs")),
        ("mirror_update.rs", include_str!("mirror_update.rs")),
        ("split_tree.rs", include_str!("split_tree.rs")),
        ("split_export.rs", include_str!("split_export.rs")),
        ("canonical_json.rs", include_str!("canonical_json.rs")),
        ("split_ci.sh", include_str!("split_ci.sh")),
        ("split_ci.yml", include_str!("split_ci.yml")),
    ] {
        let path = format!("components/jeryu-deploy/crates/jeryu-split-tool/src/{name}");
        ensure!(
            git(repo, &["show", &format!("{source}:{path}")])? == compiled,
            "compiled generator differs from source: {name}"
        );
        blobs.insert(
            path.clone(),
            git(repo, &["rev-parse", &format!("{source}:{path}")])?
                .trim()
                .to_owned(),
        );
    }
    Ok(blobs)
}

pub(super) fn prepare(request: Request<'_>) -> Result<()> {
    let Request {
        source_repo,
        source,
        component,
        export_tree,
        mirror,
        expected_tip,
        expected_tags,
        initial_tip,
    } = request;
    for id in [source, export_tree, expected_tip]
        .into_iter()
        .chain(initial_tip)
    {
        oid(id)?;
    }
    ensure!(
        component.starts_with("jeryu-")
            && component
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-'),
        "invalid component name"
    );
    let root = admit_repository(source_repo, false)?;
    let mirror = admit_repository(mirror, true)?;
    ensure!(
        mirror != root && !mirror.starts_with(&root) && !root.starts_with(&mirror),
        "source and mirror storage must be separate"
    );
    clean_source(&root, source)?;
    let source_refs = refs(&root, "refs/")?;
    let mirror_refs = refs(&mirror, "refs/")?;
    let tags = fs::read_to_string(expected_tags).context("read exact saved tag snapshot")?;
    ensure!(
        tags == refs(&mirror, "refs/tags/")?,
        "expected mirror tags changed"
    );
    let manifest: toml::Value = toml::from_str(&git(
        &root,
        &["show", &format!("{source}:repos.manifest.toml")],
    )?)?;
    let matches: Vec<_> = manifest
        .get("repo")
        .and_then(toml::Value::as_array)
        .context("source repository inventory")?
        .iter()
        .filter(|r| r.get("name").and_then(toml::Value::as_str) == Some(component))
        .collect();
    ensure!(
        matches.len() == 1,
        "component must identify one existing mirror"
    );
    let target = matches[0];
    let field = |name: &str| target.get(name).and_then(toml::Value::as_str);
    ensure!(
        field("role") == Some("split-mirror")
            && field("path") == Some(format!("components/{component}").as_str())
            && field("default_branch") == Some("main"),
        "unsupported mirror inventory entry"
    );
    ensure!(
        git(&mirror, &["rev-parse", "refs/heads/main"])?.trim() == expected_tip,
        "expected mirror main tip changed or is absent"
    );
    for repo in [&root, &mirror] {
        ensure!(
            git(repo, &["cat-file", "-t", expected_tip])?.trim() == "commit",
            "exact mirror parent must already exist in both object stores"
        );
    }
    ensure!(
        git(&root, &["cat-file", "-t", export_tree])?.trim() == "tree",
        "export must identify an existing tree"
    );
    let proposed = descriptor(&root, export_tree)?.context("export lacks split provenance")?;
    validate_provenance(&root, &proposed, component)?;
    ensure!(
        proposed.source_commit == source,
        "export binds a different source"
    );
    let generator = generator_binding(&root, source)?;
    let prior = descriptor(&mirror, expected_tip)?;
    let no_op = match prior {
        None => {
            ensure!(
                initial_tip == Some(expected_tip),
                "predecessor mirror requires explicit matching --initial-tip; new mirrors are unsupported"
            );
            false
        }
        Some(prior) => {
            ensure!(
                initial_tip.is_none(),
                "--initial-tip is only for a mirror without generated provenance"
            );
            validate_provenance(&root, &prior, component)?;
            if prior.source_commit == source {
                ensure!(
                    prior == proposed
                        && git(&mirror, &["rev-parse", &format!("{expected_tip}^{{tree}}")])?
                            .trim()
                            == export_tree,
                    "same source has different export tree or provenance"
                );
                true
            } else {
                let status = command(&root)
                    .args(["merge-base", "--is-ancestor", &prior.source_commit, source])
                    .status()?;
                ensure!(
                    status.success(),
                    "source must advance the prior source ancestry"
                );
                false
            }
        }
    };
    let source_tree = git(&root, &["rev-parse", &format!("{source}^{{tree}}")])?;
    let prepared = if no_op {
        expected_tip.to_owned()
    } else {
        let seconds = git(&root, &["show", "--no-notes", "-s", "--format=%ct", source])?;
        let seconds: u64 = seconds.trim().parse().context("source commit time")?;
        let date = format!("@{seconds} +0000");
        let message = format!(
            "Export {component} from {source}\n\nJeryu-Source-Commit: {source}\nJeryu-Source-Tree: {}\nJeryu-Component: {component}\nJeryu-Export-Tree: {export_tree}\n",
            source_tree.trim()
        );
        let output = command(&root)
            .args([
                "commit-tree",
                export_tree,
                "-p",
                expected_tip,
                "-m",
                &message,
            ])
            .env("GIT_AUTHOR_NAME", "Jeryu split export")
            .env("GIT_AUTHOR_EMAIL", "split@jeryu.invalid")
            .env("GIT_COMMITTER_NAME", "Jeryu split export")
            .env("GIT_COMMITTER_EMAIL", "split@jeryu.invalid")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()?;
        ensure!(
            output.status.success() && output.stderr.is_empty(),
            "offline successor commit preparation failed"
        );
        let id = String::from_utf8(output.stdout)?.trim().to_owned();
        oid(&id)?;
        ensure!(
            git(&root, &["show", "--no-notes", "-s", "--format=%P", &id])?.trim() == expected_tip
                && git(&root, &["rev-parse", &format!("{id}^{{tree}}")])?.trim() == export_tree,
            "prepared commit identity mismatch"
        );
        id
    };
    // Concurrent changes fail; a commit written before refusal remains unreferenced.
    clean_source(&root, source)?;
    ensure!(
        refs(&root, "refs/")? == source_refs
            && refs(&mirror, "refs/")? == mirror_refs
            && refs(&mirror, "refs/tags/")? == tags
            && fs::read_to_string(expected_tags)? == tags
            && git(&mirror, &["rev-parse", "refs/heads/main"])?.trim() == expected_tip,
        "source or expected mirror refs changed during preparation"
    );
    println!(
        "{}",
        crate::canonical_json::pretty(json!({
            "schema_version": "jeryu.offline-mirror-preparation/v1", "component": component,
            "source_commit": source, "source_tree": source_tree.trim(), "export_tree": export_tree,
            "generator_source_blobs": generator, "mirror_main_before": expected_tip,
            "mirror_tags_snapshot": tags, "prepared_commit": prepared, "no_op": no_op,
            "initial_transition": initial_tip.is_some(), "refs_changed": false,
            "publication_qualified": false, "qualification_verified": false,
            "remote_contacted": false, "object_store": root,
            "next": "independently qualify the exact export and source, then use protected forward-only publication with fresh remote readback"
        }))?
    );
    Ok(())
}
