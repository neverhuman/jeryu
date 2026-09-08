use crate::pin::Pin;
use crate::render_rules::{
    ManifestAuthority, RenderContext, ensure_script, regex, render_consumer, require_function,
    sandbox_receipt,
};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "render_auth.rs"]
mod auth;
#[path = "render_identity.rs"]
mod identity;
#[path = "render_paths.rs"]
mod paths;
#[path = "render_remote.rs"]
mod remote;
#[path = "render_storage.rs"]
mod storage;
pub(crate) use auth::git_askpass;
use auth::{
    git_local_output, git_local_output_bytes, held_askpass_executable, local_git_command,
    same_file_identity, scrub_git_environment,
};
use identity::*;
use paths::*;
use remote::*;
use storage::*;

#[derive(Default)]
struct Args {
    check: bool,
    repos: Vec<String>,
    roots: Vec<String>,
    heads: Vec<String>,
    family_root: Option<PathBuf>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => parsed.check = true,
            "--repo" | "--repo-root" | "--expected-head" | "--family-root" => {
                let flag = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{flag} requires a value"))?
                    .clone();
                match flag.as_str() {
                    "--repo" => parsed.repos.push(value),
                    "--repo-root" => parsed.roots.push(value),
                    "--expected-head" => parsed.heads.push(value),
                    "--family-root" => {
                        if parsed.family_root.replace(PathBuf::from(value)).is_some() {
                            return Err("duplicate --family-root".to_owned());
                        }
                    }
                    _ => unreachable!(),
                }
            }
            value => return Err(format!("unrecognized renderer argument: {value}")),
        }
        index += 1;
    }
    Ok(parsed)
}

pub fn run(tool_root: &Path, raw_args: &[String]) -> Result<i32, String> {
    let mut args = parse_args(raw_args)?;
    if args.repos.is_empty() && !args.check {
        args.check = true;
        eprintln!(
            "unscoped renderer invocation is check-only; pass --repo, --repo-root, and \
--expected-head for writes"
        );
    }
    let mut selected = BTreeSet::new();
    for name in &args.repos {
        if !CANONICAL_REPOS.contains(&name.as_str()) {
            return Err(format!("invalid --repo {name:?}"));
        }
        if !selected.insert(name) {
            return Err(format!("duplicate --repo {name:?}"));
        }
    }
    let pin = Pin::load(tool_root)?;
    let function = require_function(tool_root)?;
    let requested_family_root = args.family_root.unwrap_or_else(|| {
        env::var_os("JERYU_FAMILY_ROOT").map_or_else(
            || tool_root.parent().unwrap_or(tool_root).to_owned(),
            PathBuf::from,
        )
    });
    let family_root = fs::canonicalize(&requested_family_root)
        .map_err(|error| format!("failed to resolve family root: {error}"))?;
    if !args.check {
        let canonical_family_root = Path::new(CANONICAL_FAMILY_ROOT);
        if requested_family_root != canonical_family_root || family_root != canonical_family_root {
            return Err(format!(
                "renderer write mode requires the exact canonical family root: {CANONICAL_FAMILY_ROOT}"
            ));
        }
        let canonical_tool_root = canonical_family_root.join("jeryu-tool");
        if tool_root != canonical_tool_root
            || fs::canonicalize(tool_root)
                .map_err(|error| format!("failed to resolve tool root: {error}"))?
                != canonical_tool_root
        {
            return Err(format!(
                "renderer write mode requires the exact canonical tool source: {}",
                canonical_tool_root.display()
            ));
        }
    }
    let repos = if args.repos.is_empty() {
        CANONICAL_REPOS.iter().map(ToString::to_string).collect()
    } else {
        args.repos
    };
    let repo_set: BTreeSet<&str> = repos.iter().map(String::as_str).collect();
    let mut overrides = BTreeMap::new();
    for value in args.roots {
        let (name, raw_path) = value.split_once('=').ok_or_else(|| {
            format!("invalid --repo-root {value:?}; expected canonical-name=/absolute/path")
        })?;
        if !CANONICAL_REPOS.contains(&name) || raw_path.is_empty() {
            return Err(format!(
                "invalid --repo-root {value:?}; expected canonical-name=/absolute/path"
            ));
        }
        let raw_path = PathBuf::from(raw_path);
        if !raw_path.is_absolute() {
            return Err(format!(
                "invalid --repo-root {value:?}; expected canonical-name=/absolute/path"
            ));
        }
        if !args.check && raw_path != family_root.join(name) {
            return Err(format!(
                "renderer write root is not the canonical physical checkout: {name}={} expected={}",
                raw_path.display(),
                family_root.join(name).display()
            ));
        }
        let path = fs::canonicalize(&raw_path)
            .map_err(|_| format!("--repo-root is not a Git worktree: {}", raw_path.display()))?;
        if !path.join(".git").exists() {
            return Err(format!(
                "--repo-root is not a Git worktree: {}",
                path.display()
            ));
        }
        if overrides.insert(name.to_owned(), path).is_some() {
            return Err(format!("duplicate --repo-root for {name}"));
        }
    }
    let unexpected: Vec<&String> = overrides
        .keys()
        .filter(|name| !repo_set.contains(name.as_str()))
        .collect();
    if !unexpected.is_empty() {
        return Err(format!(
            "--repo-root without matching --repo: {unexpected:?}"
        ));
    }
    let mut heads = BTreeMap::new();
    if !args.check {
        let missing: Vec<&&str> = repo_set
            .iter()
            .filter(|name| !overrides.contains_key(**name))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "renderer write mode requires an explicit --repo-root for every selected repo: {missing:?}"
            ));
        }
        for value in args.heads {
            let (name, head) = value.split_once('=').ok_or_else(|| {
                format!("invalid --expected-head {value:?}; expected canonical-name=40-hex-sha")
            })?;
            if !CANONICAL_REPOS.contains(&name) || !regex("^[0-9a-f]{40}$").is_match(head) {
                return Err(format!(
                    "invalid --expected-head {value:?}; expected canonical-name=40-hex-sha"
                ));
            }
            if heads.insert(name.to_owned(), head.to_owned()).is_some() {
                return Err(format!("duplicate --expected-head for {name}"));
            }
        }
        let unexpected: Vec<&String> = heads
            .keys()
            .filter(|name| !repo_set.contains(name.as_str()))
            .collect();
        if !unexpected.is_empty() {
            return Err(format!(
                "--expected-head without matching --repo: {unexpected:?}"
            ));
        }
        let missing: Vec<&&str> = repo_set
            .iter()
            .filter(|name| !heads.contains_key(**name))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "renderer write mode requires --expected-head for every selected repo: {missing:?}"
            ));
        }
        for name in &repos {
            validate_write_root(name, &overrides[name], &family_root, &heads[name])?;
        }
    } else if !args.heads.is_empty() {
        return Err("--expected-head is valid only in explicit write mode".to_owned());
    }

    let mut repository_targets = Vec::new();
    for name in &repos {
        let root = repo_root(tool_root, &family_root, name, &overrides);
        if !root.is_dir() {
            continue;
        }
        let mut targets = consumer_paths(&root)?;
        if name == "jeryu-tool" {
            targets.push(root.join("generated/jankurai-pin.env"));
            targets.sort();
            targets.dedup();
        }
        repository_targets.push((root, targets));
    }
    if !args.check {
        validate_write_targets(&repository_targets)?;
    }

    let authority = manifest_authority(tool_root, !args.check)?;
    let receipt = sandbox_receipt(&pin, &authority)?;
    let context = RenderContext {
        authority,
        image_receipt_sha256: sha256_bytes(receipt.as_bytes())?,
    };

    let mut changed = Vec::new();
    let mut rendered_changes = Vec::new();
    if repo_set.contains("jeryu-tool") {
        let path = repo_root(tool_root, &family_root, "jeryu-tool", &overrides)
            .join("generated/jankurai-pin.env");
        if fs::read_to_string(&path).ok().as_deref() != Some(&pin.env_text()) {
            changed.push(path.clone());
            if !args.check {
                rendered_changes.push((path, pin.env_text()));
            }
        }
    }
    for (_, targets) in &repository_targets {
        for path in targets {
            if path.ends_with("generated/jankurai-pin.env") {
                continue;
            }
            let original = fs::read_to_string(path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            let rendered = render_consumer(path, &pin, &function, &context)?;
            if rendered != original {
                changed.push(path.clone());
                if !args.check {
                    rendered_changes.push((path.clone(), rendered));
                }
            }
        }
    }
    if !args.check {
        apply_rendered_changes(&repository_targets, &rendered_changes)?;
    }
    let display = |path: &Path| {
        path.strip_prefix(&family_root)
            .unwrap_or(path)
            .display()
            .to_string()
    };
    if args.check && !changed.is_empty() {
        println!("jankurai identity DRIFT — render these canonical consumers:");
        for path in changed {
            println!("  - {}", display(&path));
        }
        return Ok(1);
    }
    if args.check {
        println!(
            "jankurai identity ok: {} sha256={}",
            pin.get("version"),
            pin.get("binary_sha256")
        );
    } else if changed.is_empty() {
        println!("jankurai identity already current: {}", pin.get("version"));
    } else {
        println!(
            "rendered {} into {} file(s):",
            pin.get("version"),
            changed.len()
        );
        for path in changed {
            println!("  - {}", display(&path));
        }
    }
    Ok(0)
}

pub fn emit_ensure_script(tool_root: &Path, args: &[String]) -> Result<i32, String> {
    if !args.is_empty() {
        return Err("emit-ensure-script accepts no arguments".to_owned());
    }
    let pin = Pin::load(tool_root)?;
    let function = require_function(tool_root)?;
    print!("{}", ensure_script(&pin, &function));
    Ok(0)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
