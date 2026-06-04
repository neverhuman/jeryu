//! Affected-test planner for the fast local push lane.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use crate::GateOutcome;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AffectedPlan {
    pub base_ref: String,
    pub changed_files: Vec<String>,
    pub full_ci: bool,
    pub packages: Vec<String>,
    pub lanes: Vec<String>,
    pub workers: u32,
}

#[derive(Debug, Clone)]
struct Package {
    name: String,
    root: PathBuf,
    deps: BTreeSet<String>,
}

pub fn run_affected_plan(
    root: &Path,
    base_ref: &str,
    out: &Path,
    workers: u32,
) -> Result<GateOutcome> {
    let plan = build_affected_plan(root, base_ref, workers)?;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(root.join(parent)).with_context(|| {
            format!("create affected-plan output directory {}", parent.display())
        })?;
    }
    let out_path = root.join(out);
    fs::write(&out_path, serde_json::to_string_pretty(&plan)?)
        .with_context(|| format!("write {}", out_path.display()))?;
    Ok(GateOutcome {
        stdout: vec![
            format!("affected-plan: wrote {}", out.display()),
            format!(
                "affected-plan: changed={} packages={} lanes={} full_ci={}",
                plan.changed_files.len(),
                plan.packages.len(),
                plan.lanes.join(","),
                plan.full_ci
            ),
        ],
        exit_code: 0,
    })
}

pub fn build_affected_plan(root: &Path, base_ref: &str, workers: u32) -> Result<AffectedPlan> {
    let changed_files = changed_files(root, base_ref)?;
    let packages = workspace_packages(root)?;
    let full_ci = changed_files.iter().any(|path| escalates_to_full_ci(path));
    let lanes = affected_lanes(&changed_files, full_ci);
    let affected_packages = if full_ci {
        Vec::new()
    } else {
        affected_packages(&changed_files, &packages)
    };
    Ok(AffectedPlan {
        base_ref: base_ref.to_string(),
        changed_files,
        full_ci,
        packages: affected_packages,
        lanes,
        workers,
    })
}

fn changed_files(root: &Path, base_ref: &str) -> Result<Vec<String>> {
    let mut files = BTreeSet::new();
    let base_range = format!("{base_ref}...HEAD");
    let diff_args = [
        vec!["diff", "--name-only", "--diff-filter=ACMRTUXB", &base_range],
        vec!["diff", "--name-only", "--diff-filter=ACMRTUXB", "--cached"],
        vec!["diff", "--name-only", "--diff-filter=ACMRTUXB"],
    ];
    for (idx, args) in diff_args.iter().enumerate() {
        let changed = if idx == 0 {
            git_lines_required(root, args)?
        } else {
            git_lines(root, args)?
        };
        for file in changed {
            if !file.trim().is_empty() {
                files.insert(file);
            }
        }
    }
    for file in git_lines(root, &["ls-files", "--others", "--exclude-standard"])? {
        if !file.trim().is_empty() {
            files.insert(file);
        }
    }
    Ok(files.into_iter().collect())
}

fn git_lines(root: &Path, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn git_lines_required(root: &Path, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if !output.status.success() {
        anyhow::bail!(
            "required git command failed: git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn workspace_packages(root: &Path) -> Result<Vec<Package>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(root)
        .output()
        .context("run cargo metadata")?;
    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let workspace_members: BTreeSet<String> = metadata
        .get("workspace_members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();

    let mut workspace_names = BTreeSet::new();
    for package in metadata
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = package.get("id").and_then(Value::as_str) else {
            continue;
        };
        if workspace_members.contains(id)
            && let Some(name) = package.get("name").and_then(Value::as_str)
        {
            workspace_names.insert(name.to_string());
        }
    }

    let mut packages = Vec::new();
    for package in metadata
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = package.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !workspace_members.contains(id) {
            continue;
        }
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let manifest_path = package
            .get("manifest_path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_default();
        let root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(manifest_path);
        let deps = package
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|dep| dep.get("name").and_then(Value::as_str))
            .filter(|name| workspace_names.contains(*name))
            .map(str::to_string)
            .collect();
        packages.push(Package { name, root, deps });
    }
    Ok(packages)
}

fn affected_packages(changed_files: &[String], packages: &[Package]) -> Vec<String> {
    let mut direct = BTreeSet::new();
    for changed in changed_files {
        let changed_path = PathBuf::from(changed);
        let mut best: Option<(&str, usize)> = None;
        for package in packages {
            let package_root = package.root.strip_prefix("/").unwrap_or(&package.root);
            let relative_root = package_root
                .components()
                .skip_while(|component| {
                    component.as_os_str() != "crates" && component.as_os_str() != "bins"
                })
                .collect::<PathBuf>();
            let root_to_match = if relative_root.as_os_str().is_empty() {
                package.root.clone()
            } else {
                relative_root
            };
            if changed_path.starts_with(&root_to_match) {
                let len = root_to_match.components().count();
                if best.is_none_or(|(_, best_len)| len > best_len) {
                    best = Some((&package.name, len));
                }
            }
        }
        if let Some((name, _)) = best {
            direct.insert(name.to_string());
        }
    }

    let reverse = reverse_deps(packages);
    let mut all = direct.clone();
    let mut queue: VecDeque<_> = direct.into_iter().collect();
    while let Some(package) = queue.pop_front() {
        if let Some(dependents) = reverse.get(&package) {
            for dependent in dependents {
                if all.insert(dependent.clone()) {
                    queue.push_back(dependent.clone());
                }
            }
        }
    }
    all.into_iter().collect()
}

fn reverse_deps(packages: &[Package]) -> BTreeMap<String, BTreeSet<String>> {
    let mut reverse: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for package in packages {
        for dep in &package.deps {
            reverse
                .entry(dep.clone())
                .or_default()
                .insert(package.name.clone());
        }
    }
    reverse
}

fn affected_lanes(changed_files: &[String], full_ci: bool) -> Vec<String> {
    let mut lanes = BTreeSet::new();
    if full_ci {
        lanes.insert("full".to_string());
    }
    for path in changed_files {
        if path.starts_with("apps/web/") || path == "package.json" {
            lanes.insert("web".to_string());
        }
        if path.starts_with("crates/jeryu-tui/") {
            lanes.insert("tui".to_string());
        }
        if path.starts_with("crates/jeryu-api/") || path.starts_with("crates/jeryu-core/") {
            lanes.insert("api".to_string());
        }
        if path.starts_with("crates/jeryu-codegraph/")
            || path == "docs/codegraph-oracle.md"
            || path == "ops/ci/codegraph-oracle.sh"
        {
            lanes.insert("codegraph".to_string());
        }
        if path.starts_with("db/") {
            lanes.insert("db".to_string());
        }
        if path.starts_with("agent/") {
            lanes.insert("jankurai".to_string());
        }
    }
    lanes.into_iter().collect()
}

fn escalates_to_full_ci(path: &str) -> bool {
    path == "Cargo.toml"
        || path == "Cargo.lock"
        || path.starts_with(".github/")
        || path.starts_with("ops/")
        || path.starts_with("agent/")
        || path.starts_with("db/")
        || path.starts_with("contracts/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_roots_escalate_to_full_ci() {
        assert!(escalates_to_full_ci("Cargo.toml"));
        assert!(escalates_to_full_ci("ops/ci/full.sh"));
        assert!(escalates_to_full_ci("db/migrations/0002.sql"));
        assert!(!escalates_to_full_ci("crates/jeryu-tui/src/lib.rs"));
    }

    #[test]
    fn reverse_dependents_are_included() {
        let packages = vec![
            Package {
                name: "core".to_string(),
                root: PathBuf::from("crates/core"),
                deps: BTreeSet::new(),
            },
            Package {
                name: "api".to_string(),
                root: PathBuf::from("crates/api"),
                deps: BTreeSet::from(["core".to_string()]),
            },
        ];
        assert_eq!(
            affected_packages(&["crates/core/src/lib.rs".to_string()], &packages),
            vec!["api".to_string(), "core".to_string()]
        );
    }

    #[test]
    fn changed_files_include_untracked_worktree_files() {
        let root = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .arg("init")
            .current_dir(root.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            Command::new("git")
                .args(["config", "user.email", "ci@example.invalid"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["config", "user.name", "CI"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        fs::write(root.path().join("README.md"), "base\n").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "README.md"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "base"])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        let head = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root.path())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        assert!(
            Command::new("git")
                .args(["update-ref", "refs/remotes/origin/main", head.trim()])
                .current_dir(root.path())
                .status()
                .unwrap()
                .success()
        );
        fs::create_dir_all(root.path().join("ops/ci")).unwrap();
        fs::write(
            root.path().join("ops/ci/ci-fast.sh"),
            "#!/usr/bin/env bash\n",
        )
        .unwrap();

        assert_eq!(
            changed_files(root.path(), "origin/main").unwrap(),
            vec!["ops/ci/ci-fast.sh".to_string()]
        );
    }
}
