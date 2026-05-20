//! Owner: Repo standardizer
//! Proof: `cargo test -p jeryu --lib repo_standard`
//! Invariants: Agent-first shipping standards render deterministically and keep repo-owned policy under `.jeryu/`.

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const AGENT_FIRST_STANDARD_VERSION: &str = "agent-first-autonomous-v1";
const DEFAULT_AUTONOMY_DIR: &str = ".jeryu/autonomy";
const REQUIRED_CHECK_NAME: &str = "jeryu/required";
const JANKURAI_INSTALLER: &str = include_str!("../scripts/install-jankurai.sh");
const JANKURAI_MANIFEST: &str = include_str!("../scripts/jankurai-manifest.json");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum StandardProvider {
    Github,
    Gitlab,
}

impl StandardProvider {
    fn label(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }
}

impl fmt::Display for StandardProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoStandardMode {
    Plan,
    Apply,
    Verify,
}

#[derive(Debug, Clone)]
pub struct RepoStandardOptions {
    pub path: PathBuf,
    pub profile: String,
    pub provider: StandardProvider,
    pub base_branch: String,
    pub repo_slug: Option<String>,
    pub autonomy_dir: PathBuf,
    pub compat_autonomy_link: bool,
    pub configure_git_hooks: bool,
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct RepoStandardReport {
    pub status: RepoStandardStatus,
    pub repo_path: String,
    pub standard_version: String,
    pub profile: String,
    pub provider: StandardProvider,
    pub base_branch: String,
    pub repo_slug: String,
    pub autonomy_dir: String,
    pub required_check: String,
    pub changes: Vec<ManagedFileChange>,
    pub hook_config: HookConfigChange,
    pub legacy_autonomy_link: Option<LegacyAutonomyLinkChange>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoStandardStatus {
    Clean,
    Planned,
    Applied,
    Drift,
}

#[derive(Debug, Serialize)]
pub struct ManagedFileChange {
    pub path: String,
    pub operation: ManagedFileOperation,
    pub executable: bool,
    pub sha256: String,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedFileOperation {
    Create,
    Update,
    Unchanged,
}

#[derive(Debug, Serialize)]
pub struct HookConfigChange {
    pub desired: String,
    pub actual: Option<String>,
    pub operation: ManagedFileOperation,
}

#[derive(Debug, Serialize)]
pub struct LegacyAutonomyLinkChange {
    pub path: String,
    pub target: String,
    pub operation: ManagedFileOperation,
}

#[derive(Debug, Clone)]
struct StandardSpec {
    repo_root: PathBuf,
    profile: String,
    provider: StandardProvider,
    base_branch: String,
    repo_slug: String,
    repo_owner: String,
    repo_name: String,
    autonomy_dir: String,
}

#[derive(Debug, Clone)]
struct ManagedFile {
    path: &'static str,
    content: String,
    executable: bool,
}

pub fn run_standard(mode: RepoStandardMode, opts: RepoStandardOptions) -> Result<i32> {
    let spec = build_spec(&opts)?;
    let files = render_standard_files(&spec);
    let mut report = plan_standard(&spec, &files, &opts)?;

    match mode {
        RepoStandardMode::Plan => {
            report.status = if report_is_clean(&report) {
                RepoStandardStatus::Clean
            } else {
                RepoStandardStatus::Planned
            };
            print_report(&report, opts.json)?;
            Ok(0)
        }
        RepoStandardMode::Apply => {
            apply_standard(&spec.repo_root, &files)?;
            if opts.configure_git_hooks && spec.repo_root.join(".git").is_dir() {
                run_git(&spec.repo_root, &["config", "--local", "core.hooksPath", ".jeryu/hooks"])?;
            }
            apply_legacy_autonomy_link(&spec, &opts)?;
            report = plan_standard(&spec, &files, &opts)?;
            report.status = RepoStandardStatus::Applied;
            print_report(&report, opts.json)?;
            Ok(0)
        }
        RepoStandardMode::Verify => {
            report.status = if report_is_clean(&report) {
                RepoStandardStatus::Clean
            } else {
                RepoStandardStatus::Drift
            };
            let clean = report.status == RepoStandardStatus::Clean;
            print_report(&report, opts.json)?;
            Ok(if clean { 0 } else { 1 })
        }
    }
}

fn build_spec(opts: &RepoStandardOptions) -> Result<StandardSpec> {
    let repo_root = opts
        .path
        .canonicalize()
        .with_context(|| format!("resolving {}", opts.path.display()))?;
    if !repo_root.is_dir() {
        bail!("{} is not a directory", repo_root.display());
    }

    let repo_slug = opts
        .repo_slug
        .clone()
        .or_else(|| infer_remote_slug(&repo_root).ok().flatten())
        .unwrap_or_else(|| "unknown/unknown".to_string());
    let (repo_owner, repo_name) = split_repo_slug(&repo_slug);
    let autonomy_dir = normalize_relative_path(&opts.autonomy_dir)?;
    if !autonomy_dir.starts_with(".jeryu/") && autonomy_dir != ".jeryu" {
        bail!("autonomy-dir must live under .jeryu/; got {autonomy_dir}");
    }
    if autonomy_dir != DEFAULT_AUTONOMY_DIR {
        bail!("autonomy-dir currently must be {DEFAULT_AUTONOMY_DIR}; got {autonomy_dir}");
    }

    Ok(StandardSpec {
        repo_root,
        profile: opts.profile.clone(),
        provider: opts.provider,
        base_branch: opts.base_branch.clone(),
        repo_slug,
        repo_owner,
        repo_name,
        autonomy_dir,
    })
}

fn render_standard_files(spec: &StandardSpec) -> Vec<ManagedFile> {
    let mut files = vec![
        ManagedFile {
            path: ".jeryu/project.toml",
            content: render_project_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/delivery.toml",
            content: render_delivery_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/policies/release.toml",
            content: render_release_policy_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/policies/risk.toml",
            content: render_risk_policy_toml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/protected-paths.toml",
            content: render_protected_paths_toml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/ci/jankurai-manifest.json",
            content: ensure_trailing_newline(JANKURAI_MANIFEST),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/ci/install-jankurai.sh",
            content: ensure_trailing_newline(JANKURAI_INSTALLER),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/ci/required.sh",
            content: render_required_sh(),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/hooks/pre-push",
            content: render_pre_push_hook(&spec.base_branch),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/hooks/pre-commit",
            content: render_pre_commit_hook(),
            executable: true,
        },
        ManagedFile {
            path: ".jeryu/autonomy/autonomy.yml",
            content: render_autonomy_yml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/approvals.yml",
            content: render_autonomy_approvals_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/risk.yml",
            content: render_autonomy_risk_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/protected-paths.yml",
            content: render_autonomy_protected_paths_yml(),
            executable: false,
        },
        ManagedFile {
            path: ".jeryu/autonomy/policies/release.yml",
            content: render_autonomy_release_yml(spec),
            executable: false,
        },
        ManagedFile {
            path: ".github/workflows/jeryu-required.yml",
            content: render_github_required_workflow(),
            executable: false,
        },
        ManagedFile {
            path: ".github/CODEOWNERS",
            content: render_codeowners(spec),
            executable: false,
        },
        ManagedFile {
            path: ".github/PULL_REQUEST_TEMPLATE.md",
            content: render_pr_template(),
            executable: false,
        },
    ];

    let lock = render_standard_lock(spec, &files);
    files.push(ManagedFile {
        path: ".jeryu/standard.lock",
        content: lock,
        executable: false,
    });
    files
}

fn plan_standard(
    spec: &StandardSpec,
    files: &[ManagedFile],
    opts: &RepoStandardOptions,
) -> Result<RepoStandardReport> {
    let changes = files
        .iter()
        .map(|file| {
            let path = spec.repo_root.join(file.path);
            let operation = match fs::read_to_string(&path) {
                Ok(existing) if existing == file.content => ManagedFileOperation::Unchanged,
                Ok(_) => ManagedFileOperation::Update,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    ManagedFileOperation::Create
                }
                Err(_) => ManagedFileOperation::Update,
            };
            ManagedFileChange {
                path: file.path.to_string(),
                operation,
                executable: file.executable,
                sha256: sha256_hex(file.content.as_bytes()),
            }
        })
        .collect();

    let actual_hooks = if spec.repo_root.join(".git").is_dir() {
        git_config_get(&spec.repo_root, "core.hooksPath")?
    } else {
        None
    };
    let hook_operation = if !opts.configure_git_hooks || !spec.repo_root.join(".git").is_dir() {
        ManagedFileOperation::Unchanged
    } else if actual_hooks.as_deref() == Some(".jeryu/hooks") {
        ManagedFileOperation::Unchanged
    } else if actual_hooks.is_some() {
        ManagedFileOperation::Update
    } else {
        ManagedFileOperation::Create
    };

    Ok(RepoStandardReport {
        status: RepoStandardStatus::Planned,
        repo_path: spec.repo_root.display().to_string(),
        standard_version: AGENT_FIRST_STANDARD_VERSION.to_string(),
        profile: spec.profile.clone(),
        provider: spec.provider,
        base_branch: spec.base_branch.clone(),
        repo_slug: spec.repo_slug.clone(),
        autonomy_dir: spec.autonomy_dir.clone(),
        required_check: REQUIRED_CHECK_NAME.to_string(),
        changes,
        hook_config: HookConfigChange {
            desired: ".jeryu/hooks".to_string(),
            actual: actual_hooks,
            operation: hook_operation,
        },
        legacy_autonomy_link: plan_legacy_autonomy_link(spec, opts),
    })
}

fn apply_standard(repo_root: &Path, files: &[ManagedFile]) -> Result<()> {
    for file in files {
        let path = repo_root.join(file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        if fs::read_to_string(&path).ok().as_deref() != Some(file.content.as_str()) {
            fs::write(&path, &file.content).with_context(|| format!("writing {}", path.display()))?;
        }
        set_executable(&path, file.executable)?;
    }
    Ok(())
}

fn report_is_clean(report: &RepoStandardReport) -> bool {
    report
        .changes
        .iter()
        .all(|change| change.operation == ManagedFileOperation::Unchanged)
        && report.hook_config.operation == ManagedFileOperation::Unchanged
        && report
            .legacy_autonomy_link
            .as_ref()
            .map(|link| link.operation == ManagedFileOperation::Unchanged)
            .unwrap_or(true)
}

fn print_report(report: &RepoStandardReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!(
        "jeryu standard {} for {} ({})",
        match report.status {
            RepoStandardStatus::Clean => "clean",
            RepoStandardStatus::Planned => "plan",
            RepoStandardStatus::Applied => "applied",
            RepoStandardStatus::Drift => "drift",
        },
        report.repo_slug,
        report.repo_path
    );
    for change in &report.changes {
        if change.operation != ManagedFileOperation::Unchanged {
            println!("  {:?}: {}", change.operation, change.path);
        }
    }
    if report.hook_config.operation != ManagedFileOperation::Unchanged {
        println!(
            "  {:?}: git core.hooksPath -> {}",
            report.hook_config.operation, report.hook_config.desired
        );
    }
    if let Some(link) = &report.legacy_autonomy_link {
        if link.operation != ManagedFileOperation::Unchanged {
            println!("  {:?}: {} -> {}", link.operation, link.path, link.target);
        }
    }
    Ok(())
}

fn render_project_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nstandard = \"agent-first-autonomous\"\nstandard_version = \"{}\"\nproject_id = \"{}\"\nname = \"{}\"\ndefault_branch = \"{}\"\nstate_backend = \"redlinedb\"\ncache_policy = \"isolated\"\nmanaged_policy_root = \".jeryu\"\n",
        AGENT_FIRST_STANDARD_VERSION,
        spec.repo_slug,
        spec.repo_name,
        spec.base_branch
    )
}

fn render_delivery_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nprofile = \"{}\"\nprovider = \"{}\"\nrepo = \"{}\"\nbase_branch = \"{}\"\nautonomy_dir = \"{}\"\nrequired_check = \"{}\"\nmerge_queue_required = true\nmain_is_only_release_branch = true\nactions_must_be_pinned_to_sha = true\njob_permissions_default = \"read-only\"\ndeploy_identity = \"oidc\"\nlong_lived_deploy_credentials_allowed = false\n\n[artifact]\nbuild_once = true\npromote_same_digest = true\nrequire_signature = true\nrequire_sbom = true\nrequire_provenance = true\nrollback = \"previous_signed_digest\"\n\n[approvals]\ndefault_human_approvals = 0\nprotected_path_human_approvals = 1\ncommittee_approval_default = false\nagent_self_approval_allowed = false\n",
        spec.profile,
        spec.provider,
        spec.repo_slug,
        spec.base_branch,
        spec.autonomy_dir,
        REQUIRED_CHECK_NAME
    )
}

fn render_release_policy_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nbase_branch = \"{}\"\nrelease_branches_allowed = false\nenvironment_branches_allowed = false\nmanual_deploy_branches_allowed = false\nmerge_queue_required = true\nrequired_check = \"{}\"\n\n[build]\nsource = \"green-main\"\nonce = true\nrebuild_during_promotion = false\n\n[promotion]\nstages = [\"local\", \"dev-canary\", \"prod-limited\", \"prod-full\"]\nidentity = \"oidc\"\nverify_digest_each_stage = true\n\n[rollback]\nstrategy = \"redeploy-previous-signed-digest\"\nrebuild_allowed = false\n\n[migrations]\nstrategy = \"expand-deploy-contract\"\nbackward_compatible_release_count = 1\n",
        spec.base_branch, REQUIRED_CHECK_NAME
    )
}

fn render_risk_policy_toml() -> String {
    "schema_version = \"1\"\n\n[[tier]]\nname = \"R0\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R1\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R2\"\nhuman_approvals = 0\nagent_review_required = true\n\n[[tier]]\nname = \"R3\"\nhuman_approvals = 1\nagent_review_required = true\n\n[[tier]]\nname = \"R4\"\nhuman_approvals = 1\nagent_review_required = true\n\n[[tier]]\nname = \"R5\"\nhuman_approvals = 1\nbreak_glass_required = true\n".to_string()
}

fn render_protected_paths_toml(spec: &StandardSpec) -> String {
    format!(
        "schema_version = \"1\"\nowner = \"@{}\"\nhuman_approvals = 1\npaths = [\n  \".github/**\",\n  \".gitlab-ci.yml\",\n  \".jeryu/**\",\n  \"ops/ci/**\",\n  \"release.policy.toml\",\n  \"Cargo.lock\",\n]\n",
        spec.repo_owner
    )
}

fn render_required_sh() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"
mkdir -p target/jankurai

if ! command -v jankurai >/dev/null 2>&1; then
  echo "jeryu required: installing pinned jankurai binary" >&2
  bash .jeryu/ci/install-jankurai.sh
fi

jankurai audit . \
  --changed-fast \
  --changed-from "${JERYU_CHANGED_FROM:-origin/main}" \
  --mode advisory \
  --json target/jankurai/required-audit.json \
  --md target/jankurai/required-audit.md

if [ -x scripts/ci-parity.sh ]; then
  bash scripts/ci-parity.sh --fast --no-audit
elif command -v just >/dev/null 2>&1; then
  just fast
elif [ -f Cargo.toml ]; then
  cargo check --workspace --locked
else
  echo "jeryu required: no project proof lane found after jankurai audit" >&2
fi
"#
    .to_string()
}

fn render_pre_push_hook(base_branch: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

protected_branch="{base_branch}"
while read -r _local_ref _local_sha remote_ref _remote_sha; do
  if [ "$remote_ref" = "refs/heads/$protected_branch" ]; then
    echo "jeryu: direct push to $protected_branch is blocked; use PR plus merge queue" >&2
    exit 1
  fi
done

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
bash .jeryu/ci/required.sh
"#
    )
}

fn render_pre_commit_hook() -> String {
    r#"#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
mkdir -p target/jankurai
if command -v jankurai >/dev/null 2>&1; then
  jankurai audit . \
    --changed-fast \
    --changed-from "${JERYU_CHANGED_FROM:-origin/main}" \
    --mode advisory \
    --json target/jankurai/pre-commit-audit.json \
    --md target/jankurai/pre-commit-audit.md
else
  echo "jeryu: jankurai is not installed; run bash .jeryu/ci/install-jankurai.sh" >&2
  exit 1
fi
"#
    .to_string()
}

fn render_autonomy_yml(spec: &StandardSpec) -> String {
    format!(
        "schema: vibegate.autonomy.v1\ndefault_profile: {}\nautonomous_prod_promotion: true\npolicy_root: .jeryu/autonomy/policies\nprotected_path_policy: .jeryu/autonomy/policies/protected-paths.yml\nrelease_policy: .jeryu/autonomy/policies/release.yml\nkill_bell_required: true\nfreeze_windows_fail_closed: true\nshadow_agreement_minimum: 0.95\n",
        spec.profile
    )
}

fn render_autonomy_approvals_yml() -> String {
    "schema: vibegate.approvals.v1\ndefaults:\n  low_and_normal_risk_human_approvals: 0\n  protected_path_human_approvals: 1\n  committee_approval_default: false\n  agent_self_approval_allowed: false\ntiers:\n  R0: { human_approvals: 0, agent_review: true }\n  R1: { human_approvals: 0, agent_review: true }\n  R2: { human_approvals: 0, agent_review: true }\n  R3: { human_approvals: 1, agent_review: true }\n  R4: { human_approvals: 1, agent_review: true }\n  R5: { human_approvals: 1, break_glass: true }\n".to_string()
}

fn render_autonomy_risk_yml() -> String {
    "schema: vibegate.risk.v1\nprotected_path_tier: R4\ndefault_tier: R2\ndocs_only_tier: R0\nsmall_test_only_tier: R1\nhigh_risk_markers:\n  - .github/**\n  - .jeryu/**\n  - ops/ci/**\n  - release.policy.toml\n".to_string()
}

fn render_autonomy_protected_paths_yml() -> String {
    "schema: vibegate.protected-paths.v1\nhuman_approvals: 1\npaths:\n  - .github/**\n  - .gitlab-ci.yml\n  - .jeryu/**\n  - ops/ci/**\n  - release.policy.toml\n  - Cargo.lock\n".to_string()
}

fn render_autonomy_release_yml(spec: &StandardSpec) -> String {
    format!(
        "schema: vibegate.release.v1\ncanonical_branch: {}\nrequired_check: {}\nmerge_queue_required: true\nbuild_once: true\npromote_same_digest: true\nsignature_required: true\nsbom_required: true\nprovenance_required: true\nrollback_strategy: previous_signed_digest\nstages:\n  - local\n  - dev-canary\n  - prod-limited\n  - prod-full\n",
        spec.base_branch, REQUIRED_CHECK_NAME
    )
}

fn render_github_required_workflow() -> String {
    r#"name: jeryu required

on:
  pull_request:
  merge_group:

permissions:
  contents: read

jobs:
  required:
    name: jeryu/required
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6
        with:
          fetch-depth: 0
      - name: Required lane
        run: bash .jeryu/ci/required.sh
"#
    .to_string()
}

fn render_codeowners(spec: &StandardSpec) -> String {
    format!(
        "* @{}\n.github/** @{}\n.jeryu/** @{}\nops/ci/** @{}\n",
        spec.repo_owner, spec.repo_owner, spec.repo_owner, spec.repo_owner
    )
}

fn render_pr_template() -> String {
    r#"## Change Surface
- [ ] R0 docs/comment-only
- [ ] R1 small fix or test-only
- [ ] R2 normal product change
- [ ] R3 runtime, dependency, data, or API behavior
- [ ] R4 protected path: .github, .jeryu, release, security, CI
- [ ] R5 break-glass or production incident

## Evidence
- [ ] `jeryu/required` passed
- [ ] Independent agent review receipt attached or linked
- [ ] Rollback path unchanged or tested
- [ ] Same artifact digest is promoted across environments when release-bound
"#
    .to_string()
}

fn render_standard_lock(spec: &StandardSpec, files: &[ManagedFile]) -> String {
    let mut out = format!(
        "schema_version = \"1\"\nstandard_version = \"{}\"\nprovider = \"{}\"\nrepo = \"{}\"\nbase_branch = \"{}\"\nautonomy_dir = \"{}\"\n\n",
        AGENT_FIRST_STANDARD_VERSION,
        spec.provider,
        spec.repo_slug,
        spec.base_branch,
        spec.autonomy_dir
    );
    for file in files {
        out.push_str("[[managed_file]]\n");
        out.push_str(&format!("path = \"{}\"\n", file.path));
        out.push_str(&format!("sha256 = \"{}\"\n", sha256_hex(file.content.as_bytes())));
        out.push_str(&format!("executable = {}\n\n", file.executable));
    }
    out
}

fn plan_legacy_autonomy_link(
    spec: &StandardSpec,
    opts: &RepoStandardOptions,
) -> Option<LegacyAutonomyLinkChange> {
    if !opts.compat_autonomy_link {
        return None;
    }
    let path = spec.repo_root.join(".autonomy");
    let target = spec.autonomy_dir.clone();
    let operation = if path.exists() {
        ManagedFileOperation::Unchanged
    } else {
        ManagedFileOperation::Create
    };
    Some(LegacyAutonomyLinkChange {
        path: ".autonomy".to_string(),
        target,
        operation,
    })
}

fn apply_legacy_autonomy_link(spec: &StandardSpec, opts: &RepoStandardOptions) -> Result<()> {
    if !opts.compat_autonomy_link {
        return Ok(());
    }
    let link = spec.repo_root.join(".autonomy");
    if link.exists() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&spec.autonomy_dir, &link)
            .with_context(|| format!("creating {}", link.display()))?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&link, &spec.autonomy_dir).with_context(|| format!("writing {}", link.display()))?;
    }
    Ok(())
}

fn infer_remote_slug(repo_root: &Path) -> Result<Option<String>> {
    let Some(remote) = git_config_get(repo_root, "remote.origin.url")? else {
        return Ok(None);
    };
    Ok(parse_remote_slug(&remote))
}

fn parse_remote_slug(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("git@") {
        let (_, path) = rest.split_once(':')?;
        return normalize_slug(path);
    }
    if let Some((_, path)) = trimmed.split_once("://") {
        let mut parts = path.split('/').collect::<Vec<_>>();
        if parts.len() >= 3 {
            let repo = parts.pop()?;
            let owner = parts.pop()?;
            return normalize_slug(&format!("{owner}/{repo}"));
        }
    }
    normalize_slug(trimmed)
}

fn normalize_slug(value: &str) -> Option<String> {
    let mut parts = value
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let repo = parts.pop()?.trim_end_matches(".git");
    let owner = parts.pop()?;
    Some(format!("{owner}/{repo}"))
}

fn split_repo_slug(slug: &str) -> (String, String) {
    let mut parts = slug.split('/');
    let owner = parts.next().unwrap_or("repo-owner").to_string();
    let name = parts.next().unwrap_or("repo").to_string();
    (owner, name)
}

fn normalize_relative_path(path: &Path) -> Result<String> {
    if path.is_absolute() {
        bail!("path must be repo-relative: {}", path.display());
    }
    let value = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();
    if value.is_empty() {
        bail!("path must not be empty");
    }
    Ok(value)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn ensure_trailing_newline(input: &str) -> String {
    if input.ends_with('\n') {
        input.to_string()
    } else {
        format!("{input}\n")
    }
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(if executable { 0o755 } else { 0o644 });
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

fn git_config_get(repo_root: &Path, key: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "--get", key])
        .output()
        .with_context(|| format!("reading git config {key}"))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("running git {:?}", args))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_remote_slug_accepts_common_git_urls() {
        assert_eq!(
            parse_remote_slug("git@github.com:neverhuman/warp.git").as_deref(),
            Some("neverhuman/warp")
        );
        assert_eq!(
            parse_remote_slug("https://github.com/neverhuman/warp.git").as_deref(),
            Some("neverhuman/warp")
        );
    }

    #[test]
    fn render_keeps_managed_policy_under_jeryu_except_host_integrations() {
        let spec = StandardSpec {
            repo_root: PathBuf::from("."),
            profile: "sovereign_plus".to_string(),
            provider: StandardProvider::Github,
            base_branch: "main".to_string(),
            repo_slug: "neverhuman/warp".to_string(),
            repo_owner: "neverhuman".to_string(),
            repo_name: "warp".to_string(),
            autonomy_dir: DEFAULT_AUTONOMY_DIR.to_string(),
        };
        let files = render_standard_files(&spec);
        assert!(files.iter().any(|file| file.path == ".jeryu/standard.lock"));
        for file in files {
            assert!(
                file.path.starts_with(".jeryu/")
                    || file.path.starts_with(".github/")
                    || file.path == ".gitlab-ci.yml",
                "unexpected managed path outside .jeryu/host integration: {}",
                file.path
            );
        }
    }

    #[test]
    fn apply_then_verify_is_clean_in_temp_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        run_git(tmp.path(), &["init", "-b", "main"]).unwrap();
        run_git(
            tmp.path(),
            &["remote", "add", "origin", "git@github.com:neverhuman/warp.git"],
        )
        .unwrap();

        let opts = RepoStandardOptions {
            path: tmp.path().to_path_buf(),
            profile: "sovereign_plus".to_string(),
            provider: StandardProvider::Github,
            base_branch: "main".to_string(),
            repo_slug: None,
            autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
            compat_autonomy_link: true,
            configure_git_hooks: true,
            json: true,
        };

        assert_eq!(run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(), 0);
        assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 0);
        assert!(tmp.path().join(".jeryu/project.toml").is_file());
        assert!(tmp.path().join(".jeryu/standard.lock").is_file());
        assert_eq!(
            git_config_get(tmp.path(), "core.hooksPath").unwrap().as_deref(),
            Some(".jeryu/hooks")
        );
    }

    #[test]
    fn veox_hard_switch_repo_infers_remote_slug_and_writes_jeryu_policy() {
        let tmp = tempfile::tempdir().unwrap();
        run_git(tmp.path(), &["init", "-b", "main"]).unwrap();
        run_git(
            tmp.path(),
            &["remote", "add", "origin", "git@github.com:neverhuman/warp.git"],
        )
        .unwrap();

        fs::create_dir_all(tmp.path().join(".jeryu")).unwrap();
        fs::write(
            tmp.path().join(".jeryu/delivery.toml"),
            "schema_version = \"1\"\nrepo = \"stale-owner/stale-repo\"\n",
        )
        .unwrap();
        fs::create_dir_all(tmp.path().join(".autonomy/policies")).unwrap();
        fs::write(
            tmp.path().join(".autonomy/autonomy.yml"),
            "schema: vibegate.autonomy.v1\npolicy_root: .autonomy/policies\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join(".autonomy/policies/release.yml"),
            "schema: vibegate.release.v1\ncanonical_branch: trunk\n",
        )
        .unwrap();

        let opts = RepoStandardOptions {
            path: tmp.path().to_path_buf(),
            profile: "sovereign_plus".to_string(),
            provider: StandardProvider::Github,
            base_branch: "main".to_string(),
            repo_slug: None,
            autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
            compat_autonomy_link: false,
            configure_git_hooks: false,
            json: true,
        };

        let spec = build_spec(&opts).unwrap();
        assert_eq!(spec.repo_slug, "neverhuman/warp");
        let files = render_standard_files(&spec);
        let delivery = files
            .iter()
            .find(|file| file.path == ".jeryu/delivery.toml")
            .unwrap();
        assert!(delivery.content.contains("repo = \"neverhuman/warp\""));
        let plan = plan_standard(&spec, &files, &opts).unwrap();
        assert_eq!(plan.repo_slug, "neverhuman/warp");
        assert!(plan.legacy_autonomy_link.is_none());
        assert_eq!(
            plan.changes
                .iter()
                .find(|change| change.path == ".jeryu/delivery.toml")
                .unwrap()
                .operation,
            ManagedFileOperation::Update
        );
        assert_eq!(run_standard(RepoStandardMode::Plan, opts.clone()).unwrap(), 0);

        assert_eq!(run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(), 0);
        let rendered_delivery =
            fs::read_to_string(tmp.path().join(".jeryu/delivery.toml")).unwrap();
        assert!(rendered_delivery.contains("repo = \"neverhuman/warp\""));
        assert!(!rendered_delivery.contains("stale-owner/stale-repo"));

        for path in [
            ".jeryu/autonomy/autonomy.yml",
            ".jeryu/autonomy/policies/approvals.yml",
            ".jeryu/autonomy/policies/protected-paths.yml",
            ".jeryu/autonomy/policies/release.yml",
            ".jeryu/autonomy/policies/risk.yml",
            ".github/CODEOWNERS",
            ".github/workflows/jeryu-required.yml",
        ] {
            assert!(tmp.path().join(path).is_file(), "missing {path}");
        }
        let rendered_autonomy =
            fs::read_to_string(tmp.path().join(".jeryu/autonomy/autonomy.yml")).unwrap();
        assert!(rendered_autonomy.contains("policy_root: .jeryu/autonomy/policies"));
        assert!(!rendered_autonomy.contains("policy_root: .autonomy/policies"));
        let rendered_release =
            fs::read_to_string(tmp.path().join(".jeryu/autonomy/policies/release.yml")).unwrap();
        assert!(rendered_release.contains("canonical_branch: main"));
        assert!(!rendered_release.contains("canonical_branch: trunk"));
        assert!(
            !fs::symlink_metadata(tmp.path().join(".autonomy"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(tmp.path().join(".github/CODEOWNERS"))
            .unwrap()
            .contains("@neverhuman"));
        assert!(
            fs::read_to_string(tmp.path().join(".github/workflows/jeryu-required.yml"))
                .unwrap()
                .contains("jeryu/required")
        );

        let spec = build_spec(&opts).unwrap();
        let files = render_standard_files(&spec);
        let clean_plan = plan_standard(&spec, &files, &opts).unwrap();
        assert!(report_is_clean(&clean_plan));
        assert_eq!(run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(), 0);
        assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 0);
    }

    #[test]
    fn verify_reports_drift_when_managed_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let opts = RepoStandardOptions {
            path: tmp.path().to_path_buf(),
            profile: "sovereign_plus".to_string(),
            provider: StandardProvider::Github,
            base_branch: "main".to_string(),
            repo_slug: Some("neverhuman/warp".to_string()),
            autonomy_dir: PathBuf::from(DEFAULT_AUTONOMY_DIR),
            compat_autonomy_link: false,
            configure_git_hooks: false,
            json: true,
        };

        assert_eq!(run_standard(RepoStandardMode::Apply, opts.clone()).unwrap(), 0);
        fs::write(tmp.path().join(".jeryu/project.toml"), "drift\n").unwrap();
        assert_eq!(run_standard(RepoStandardMode::Verify, opts).unwrap(), 1);
    }
}
