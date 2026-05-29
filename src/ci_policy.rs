//! Owner: CI Platform Policy Doctor
//! Proof: `cargo test -p jeryu --lib ci_policy`
//! Invariants: Standard GitLab CI is untagged-only; doctor is read-only.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct CiPolicyReport {
    pub generated_at: String,
    pub repo: String,
    pub ok: bool,
    pub config_path: String,
    pub config: Option<CiPolicyConfig>,
    pub files_checked: Vec<String>,
    pub findings: Vec<CiPolicyFinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CiPolicyFleetReport {
    pub generated_at: String,
    pub ok: bool,
    pub repos: Vec<CiPolicyReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CiPolicyFinding {
    pub code: String,
    pub severity: String,
    pub path: String,
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CiPolicyConfig {
    pub schema_version: Option<String>,
    #[serde(default)]
    pub github_actions_required: bool,
    #[serde(default)]
    pub local_gitlab_required: bool,
    pub gitlab_required_job: Option<String>,
    #[serde(default)]
    pub runner: CiRunnerPolicy,
    #[serde(default)]
    pub image: CiImagePolicy,
    #[serde(default)]
    pub cache: CiCachePolicy,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CiRunnerPolicy {
    #[serde(default = "default_runner_policy")]
    pub policy: String,
    #[serde(default = "default_true")]
    pub forbid_tags: bool,
    #[serde(default)]
    pub standard_jobs_untagged: bool,
}

impl Default for CiRunnerPolicy {
    fn default() -> Self {
        Self {
            policy: default_runner_policy(),
            forbid_tags: true,
            standard_jobs_untagged: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CiImagePolicy {
    #[serde(default = "default_rust_image")]
    pub rust: String,
    #[serde(default = "default_true")]
    pub pinned: bool,
}

impl Default for CiImagePolicy {
    fn default() -> Self {
        Self {
            rust: default_rust_image(),
            pinned: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CiCachePolicy {
    #[serde(default = "default_cache_policy")]
    pub policy: String,
    #[serde(default = "default_true")]
    pub cargo_jeryu_managed: bool,
    #[serde(default = "default_true")]
    pub project_sccache_disabled: bool,
}

impl Default for CiCachePolicy {
    fn default() -> Self {
        Self {
            policy: default_cache_policy(),
            cargo_jeryu_managed: true,
            project_sccache_disabled: true,
        }
    }
}

pub fn doctor_repo(repo: &Path) -> Result<CiPolicyReport> {
    let repo = repo
        .canonicalize()
        .with_context(|| format!("resolving repo {}", repo.display()))?;
    let config_path = repo.join(".jeryu/ci.toml");
    let mut findings = Vec::new();
    let config = load_ci_policy_config(&config_path, &mut findings)?;

    if let Some(config) = &config {
        validate_config_policy(config, &config_path, &mut findings);
    }

    if config
        .as_ref()
        .map(|config| config.github_actions_required)
        .unwrap_or(false)
        && !has_github_workflows(&repo)?
    {
        findings.push(finding(
            "missing_github_actions",
            "error",
            &repo.join(".github/workflows"),
            None,
            "GitHub Actions are required but no workflow YAML files were found".to_string(),
        ));
    }

    let mut files_checked = Vec::new();
    let mut visited = BTreeSet::new();
    let root_ci = repo.join(".gitlab-ci.yml");
    if root_ci.exists() {
        scan_gitlab_ci_file(
            &repo,
            &root_ci,
            &mut visited,
            &mut files_checked,
            &mut findings,
        )?;
    } else if config
        .as_ref()
        .map(|config| config.local_gitlab_required)
        .unwrap_or(false)
    {
        findings.push(finding(
            "missing_gitlab_ci",
            "error",
            &root_ci,
            None,
            "local GitLab is required but .gitlab-ci.yml is missing".to_string(),
        ));
    }
    scan_additional_policy_files(&repo, &mut visited, &mut files_checked, &mut findings)?;

    let ok = findings.iter().all(|finding| finding.severity != "error");
    Ok(CiPolicyReport {
        generated_at: now_rfc3339(),
        repo: repo.display().to_string(),
        ok,
        config_path: config_path.display().to_string(),
        config,
        files_checked,
        findings,
    })
}

pub fn doctor_default_fleet() -> Result<CiPolicyFleetReport> {
    let mut repos = BTreeSet::new();
    for entry in crate::access::load_contract()?.known_repos {
        repos.insert(PathBuf::from(entry.path));
    }
    for repo in default_policy_repos() {
        repos.insert(repo);
    }
    doctor_fleet_paths(repos)
}

fn doctor_fleet_paths<I>(paths: I) -> Result<CiPolicyFleetReport>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut reports = Vec::new();
    for repo in paths {
        if repo.is_dir() {
            reports.push(doctor_repo(&repo)?);
        } else {
            reports.push(missing_repo_report(&repo));
        }
    }
    let ok = reports.iter().all(|report| report.ok);
    Ok(CiPolicyFleetReport {
        generated_at: now_rfc3339(),
        ok,
        repos: reports,
    })
}

fn missing_repo_report(repo: &Path) -> CiPolicyReport {
    CiPolicyReport {
        generated_at: now_rfc3339(),
        repo: repo.display().to_string(),
        ok: false,
        config_path: repo.join(".jeryu/ci.toml").display().to_string(),
        config: None,
        files_checked: Vec::new(),
        findings: vec![finding(
            "repo_path_missing",
            "error",
            repo,
            None,
            format!(
                "registered CI policy repo path does not exist: {}",
                repo.display()
            ),
        )],
    }
}

pub fn render_rust_gitlab_ci_template() -> String {
    let policy = CiPolicyConfig::default();
    format!(
        r#"image: {}

stages:
  - test

variables:
  CARGO_HOME: "$CI_PROJECT_DIR/.jeryu-cache/cargo-home"
  RUSTUP_HOME: "$CI_PROJECT_DIR/.jeryu-cache/rustup-home"
  CARGO_TARGET_DIR: "$CI_PROJECT_DIR/target"
  SCCACHE_DISABLE: "1"

default:
  interruptible: true
  before_script:
    - rustc --version
    - cargo --version

check:
  stage: test
  script:
    - cargo check --workspace --locked
"#,
        policy.image.rust
    )
}

fn load_ci_policy_config(
    path: &Path,
    findings: &mut Vec<CiPolicyFinding>,
) -> Result<Option<CiPolicyConfig>> {
    if !path.exists() {
        findings.push(finding(
            "missing_ci_policy",
            "warning",
            path,
            None,
            ".jeryu/ci.toml is missing; CI policy defaults cannot be verified".to_string(),
        ));
        return Ok(None);
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    match toml::from_str::<CiPolicyConfig>(&text) {
        Ok(config) => Ok(Some(config)),
        Err(err) => {
            findings.push(finding(
                "invalid_ci_policy",
                "error",
                path,
                None,
                format!("could not parse .jeryu/ci.toml: {err}"),
            ));
            Ok(None)
        }
    }
}

fn validate_config_policy(
    config: &CiPolicyConfig,
    path: &Path,
    findings: &mut Vec<CiPolicyFinding>,
) {
    if config.local_gitlab_required && !config.runner.forbid_tags {
        findings.push(finding(
            "runner_tags_allowed",
            "error",
            path,
            None,
            "local GitLab policy requires runner.forbid_tags=true".to_string(),
        ));
    }
    if config.runner.policy != "untagged-only" {
        findings.push(finding(
            "runner_policy_drift",
            "error",
            path,
            None,
            format!(
                "runner.policy must be \"untagged-only\", got {:?}",
                config.runner.policy
            ),
        ));
    }
    if !config.runner.standard_jobs_untagged {
        findings.push(finding(
            "standard_jobs_tagged",
            "error",
            path,
            None,
            "runner.standard_jobs_untagged must be true".to_string(),
        ));
    }
    if !config.image.pinned || config.image.rust.ends_with(":latest") {
        findings.push(finding(
            "unpinned_rust_image",
            "error",
            path,
            None,
            format!("Rust image must be pinned, got {:?}", config.image.rust),
        ));
    }
    if !config.cache.cargo_jeryu_managed {
        findings.push(finding(
            "cargo_cache_policy_drift",
            "warning",
            path,
            None,
            "cargo cache should be Jeryu-managed".to_string(),
        ));
    }
}

fn scan_gitlab_ci_file(
    repo: &Path,
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files_checked: &mut Vec<String>,
    findings: &mut Vec<CiPolicyFinding>,
) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving CI file {}", path.display()))?;
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    files_checked.push(canonical.display().to_string());
    let text = fs::read_to_string(&canonical)
        .with_context(|| format!("reading {}", canonical.display()))?;
    scan_for_tags_lines(&canonical, &text, findings);
    scan_for_agent_bypass_lines(&canonical, &text, findings);

    let yaml = match serde_yaml::from_str::<serde_yaml::Value>(&text) {
        Ok(value) => value,
        Err(err) => {
            findings.push(finding(
                "invalid_gitlab_ci_yaml",
                "error",
                &canonical,
                None,
                format!("could not parse GitLab CI YAML: {err}"),
            ));
            return Ok(());
        }
    };
    scan_critical_gitlab_jobs(&canonical, &yaml, findings);

    for include in local_includes(&yaml) {
        for include_path in expand_local_include(repo, &include)? {
            if include_path.exists() {
                scan_gitlab_ci_file(repo, &include_path, visited, files_checked, findings)?;
            } else {
                findings.push(finding(
                    "missing_ci_include",
                    "error",
                    &include_path,
                    None,
                    format!("local include {include:?} does not exist"),
                ));
            }
        }
    }
    Ok(())
}

fn scan_additional_policy_files(
    repo: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files_checked: &mut Vec<String>,
    findings: &mut Vec<CiPolicyFinding>,
) -> Result<()> {
    for path in collect_policy_scan_files(repo)? {
        scan_policy_file(&path, visited, files_checked, findings)?;
    }
    Ok(())
}

fn scan_policy_file(
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files_checked: &mut Vec<String>,
    findings: &mut Vec<CiPolicyFinding>,
) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving policy file {}", path.display()))?;
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    files_checked.push(canonical.display().to_string());
    let text = fs::read_to_string(&canonical)
        .with_context(|| format!("reading {}", canonical.display()))?;
    scan_for_agent_bypass_lines(&canonical, &text, findings);
    if is_github_workflow(&canonical) {
        scan_for_github_workflow_direct_push(&canonical, &text, findings);
    }
    Ok(())
}

fn scan_for_tags_lines(path: &Path, text: &str, findings: &mut Vec<CiPolicyFinding>) {
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed == "tags:" || trimmed.starts_with("tags: ") || trimmed.starts_with("tags:[") {
            findings.push(finding(
                "forbidden_runner_tags",
                "error",
                path,
                Some(index + 1),
                "GitLab CI defines runner tags; standard jobs must stay untagged".to_string(),
            ));
        }
    }
}

fn scan_for_agent_bypass_lines(path: &Path, text: &str, findings: &mut Vec<CiPolicyFinding>) {
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower == "glab" || lower.starts_with("glab ") || lower.contains(" glab ") {
            findings.push(finding(
                "forbidden_glab",
                "error",
                path,
                Some(index + 1),
                "CI/agent scripts must use Jeryu APIs or CLI, not glab".to_string(),
            ));
        }
        if lower.contains("/api/v4")
            && (lower.contains("gitlab.local:8929")
                || lower.contains("127.0.0.1:8929")
                || lower.contains("localhost:8929"))
        {
            findings.push(finding(
                "raw_local_gitlab_api",
                "error",
                path,
                Some(index + 1),
                "raw local GitLab REST calls bypass Jeryu access repair and policy surfaces"
                    .to_string(),
            ));
        }
        if trimmed.contains("GITLAB_TOKEN")
            || trimmed.contains("GITLAB_PAT")
            || trimmed.contains("PRIVATE_TOKEN")
            || trimmed.contains("PRIVATE-TOKEN")
        {
            findings.push(finding(
                "local_gitlab_token_bypass",
                "error",
                path,
                Some(index + 1),
                "direct local GitLab token usage is forbidden; use Jeryu auth/access helpers"
                    .to_string(),
            ));
        }
        if lower.contains("gitlab-ci-token:") && lower.contains('@') {
            findings.push(finding(
                "token_bearing_git_remote",
                "error",
                path,
                Some(index + 1),
                "do not construct token-bearing GitLab clone URLs; use SSH access or askpass"
                    .to_string(),
            ));
        }
    }
}

fn scan_for_github_workflow_direct_push(
    path: &Path,
    text: &str,
    findings: &mut Vec<CiPolicyFinding>,
) {
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if trimmed == "git push" || trimmed.starts_with("git push ") || trimmed.contains("git push")
        {
            findings.push(finding(
                "github_workflow_direct_push",
                "error",
                path,
                Some(index + 1),
                "GitHub workflows must not push directly to protected branches; upload artifacts or route through Jeryu-controlled review"
                    .to_string(),
            ));
        }
    }
}

fn scan_critical_gitlab_jobs(
    path: &Path,
    value: &serde_yaml::Value,
    findings: &mut Vec<CiPolicyFinding>,
) {
    let Some(mapping) = value.as_mapping() else {
        return;
    };
    for job in CRITICAL_GITLAB_JOBS {
        let key = serde_yaml::Value::String((*job).to_string());
        let Some(job_value) = mapping.get(&key) else {
            continue;
        };
        if allow_failure_is_true(job_value) {
            findings.push(finding(
                "critical_ci_lane_fail_open",
                "error",
                path,
                None,
                format!("{job} is a critical lane and must not set allow_failure: true"),
            ));
        }
    }
}

fn allow_failure_is_true(value: &serde_yaml::Value) -> bool {
    let Some(mapping) = value.as_mapping() else {
        return false;
    };
    let key = serde_yaml::Value::String("allow_failure".to_string());
    match mapping.get(&key) {
        Some(serde_yaml::Value::Bool(true)) => true,
        Some(serde_yaml::Value::String(value)) => value.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

const CRITICAL_GITLAB_JOBS: &[&str] = &[
    "gitlab_source_auth_doctor",
    "jankurai_security",
    "jankurai_proof",
    "jankurai_tools",
    "jankurai_bad_behavior",
    "jankurai_sbom",
];

fn local_includes(value: &serde_yaml::Value) -> Vec<String> {
    let mut includes = Vec::new();
    let Some(mapping) = value.as_mapping() else {
        return includes;
    };
    let key = serde_yaml::Value::String("include".to_string());
    let Some(include_value) = mapping.get(&key) else {
        return includes;
    };
    collect_local_includes(include_value, &mut includes);
    includes
}

fn collect_local_includes(value: &serde_yaml::Value, includes: &mut Vec<String>) {
    match value {
        serde_yaml::Value::String(path) => includes.push(path.clone()),
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                collect_local_includes(item, includes);
            }
        }
        serde_yaml::Value::Mapping(mapping) => {
            let local_key = serde_yaml::Value::String("local".to_string());
            if let Some(local) = mapping.get(&local_key) {
                collect_local_includes(local, includes);
            }
        }
        _ => {}
    }
}

fn collect_policy_scan_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in [
        repo.join(".github/workflows"),
        repo.join("ops/ci"),
        repo.join("scripts"),
        repo.join("tools"),
    ] {
        if !root.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "target" | "node_modules")
                )
            })
        {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if is_policy_scan_candidate(repo, path) {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_policy_scan_candidate(repo: &Path, path: &Path) -> bool {
    if path.starts_with(repo.join(".github/workflows")) {
        return matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yml" | "yaml")
        );
    }
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("sh"))
}

fn has_github_workflows(repo: &Path) -> Result<bool> {
    let workflows = repo.join(".github/workflows");
    if !workflows.is_dir() {
        return Ok(false);
    }
    for entry in
        fs::read_dir(&workflows).with_context(|| format!("reading {}", workflows.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file()
            && matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("yml" | "yaml")
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_github_workflow(path: &Path) -> bool {
    path.to_string_lossy().contains("/.github/workflows/")
}

fn expand_local_include(repo: &Path, include: &str) -> Result<Vec<PathBuf>> {
    if !include.contains('*') && !include.contains('?') {
        return Ok(vec![repo.join(include.trim_start_matches('/'))]);
    }
    let pattern = include.trim_start_matches('/').to_string();
    let mut matches = Vec::new();
    for entry in walkdir::WalkDir::new(repo)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | "target" | "node_modules")
            )
        })
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(repo).unwrap_or(entry.path());
        let relative = relative.to_string_lossy().replace('\\', "/");
        if wildcard_match(&pattern, &relative) {
            matches.push(entry.path().to_path_buf());
        }
    }
    Ok(matches)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    wildcard_match_bytes(pattern.as_bytes(), value.as_bytes())
}

fn wildcard_match_bytes(pattern: &[u8], value: &[u8]) -> bool {
    if pattern.is_empty() {
        return value.is_empty();
    }
    match pattern[0] {
        b'*' => {
            wildcard_match_bytes(&pattern[1..], value)
                || (!value.is_empty() && wildcard_match_bytes(pattern, &value[1..]))
        }
        b'?' => !value.is_empty() && wildcard_match_bytes(&pattern[1..], &value[1..]),
        ch => {
            !value.is_empty() && ch == value[0] && wildcard_match_bytes(&pattern[1..], &value[1..])
        }
    }
}

fn default_policy_repos() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/ubuntu"));
    let mut repos = vec![
        home.join("jeryu"),
        home.join("redlineDB"),
        home.join("redline-testing"),
        home.join("jekko"),
        home.join("jankurai"),
        home.join("jansu"),
        home.join("openQG"),
    ];
    let veox_root = home.join("veox-split");
    if let Ok(entries) = fs::read_dir(&veox_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                repos.push(path);
            }
        }
    }
    repos
}

fn finding(
    code: &str,
    severity: &str,
    path: &Path,
    line: Option<usize>,
    message: String,
) -> CiPolicyFinding {
    CiPolicyFinding {
        code: code.to_string(),
        severity: severity.to_string(),
        path: path.display().to_string(),
        line,
        message,
    }
}

fn default_runner_policy() -> String {
    "untagged-only".to_string()
}

fn default_rust_image() -> String {
    "rust:1.95.0".to_string()
}

fn default_cache_policy() -> String {
    "jeryu-managed".to_string()
}

fn default_true() -> bool {
    true
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tags_in_root_includes_and_anchors() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("ci")).unwrap();
        fs::create_dir_all(root.join(".jeryu")).unwrap();
        fs::write(
            root.join(".jeryu/ci.toml"),
            r#"schema_version = "1"
github_actions_required = false
local_gitlab_required = true

[runner]
policy = "untagged-only"
forbid_tags = true
standard_jobs_untagged = true

[image]
rust = "rust:1.95.0"
pinned = true

[cache]
policy = "jeryu-managed"
cargo_jeryu_managed = true
project_sccache_disabled = true
"#,
        )
        .unwrap();
        fs::write(
            root.join(".gitlab-ci.yml"),
            r#"include:
  - local: ci/*.yml

.tagged_template: &tagged_template
  tags: [default]
"#,
        )
        .unwrap();
        fs::write(
            root.join("ci/build.yml"),
            r#"build:
  script: cargo check
  tags:
    - docker-build
"#,
        )
        .unwrap();

        let report = doctor_repo(root).unwrap();

        assert!(!report.ok);
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.code == "forbidden_runner_tags")
                .count(),
            2
        );
        assert_eq!(report.files_checked.len(), 2);
    }

    #[test]
    fn policy_config_requires_pinned_untagged_runners() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".jeryu")).unwrap();
        fs::write(
            root.join(".jeryu/ci.toml"),
            r#"schema_version = "1"
local_gitlab_required = true

[runner]
policy = "tagged"
forbid_tags = false
standard_jobs_untagged = false

[image]
rust = "rust:latest"
pinned = false
"#,
        )
        .unwrap();
        fs::write(
            root.join(".gitlab-ci.yml"),
            "check:\n  script: cargo check\n",
        )
        .unwrap();

        let report = doctor_repo(root).unwrap();

        assert!(!report.ok);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == "runner_policy_drift")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.code == "unpinned_rust_image")
        );
    }

    #[test]
    fn rust_template_has_no_runner_tags() {
        let template = render_rust_gitlab_ci_template();
        assert!(!template.contains("tags:"));
        assert!(template.contains("rust:1.95.0"));
    }

    #[test]
    fn fleet_doctor_reports_missing_registered_paths() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing-repo");

        let report = doctor_fleet_paths(vec![missing.clone()]).unwrap();

        assert!(!report.ok);
        assert_eq!(report.repos.len(), 1);
        assert_eq!(report.repos[0].repo, missing.display().to_string());
        assert!(
            report.repos[0]
                .findings
                .iter()
                .any(|finding| finding.code == "repo_path_missing")
        );
    }

    #[test]
    fn detects_raw_gitlab_bypass_and_direct_github_pushes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".jeryu")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(
            root.join(".jeryu/ci.toml"),
            r#"schema_version = "1"
github_actions_required = true
local_gitlab_required = true

[runner]
policy = "untagged-only"
forbid_tags = true
standard_jobs_untagged = true

[image]
rust = "rust:1.95.0"
pinned = true
"#,
        )
        .unwrap();
        fs::write(
            root.join(".gitlab-ci.yml"),
            r#"ci_runner_policy:
  script: echo ok
gitlab_source_auth_doctor:
  script: bash scripts/check.sh
jankurai_security:
  script: bash ops/ci/jankurai-lane.sh security
"#,
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/rust.yml"),
            r#"name: Rust
jobs:
  docs:
    steps:
      - run: git push
"#,
        )
        .unwrap();
        fs::write(
            root.join("scripts/check.sh"),
            r#"#!/usr/bin/env bash
GITLAB_TOKEN=bad curl http://gitlab.local:8929/api/v4/projects
git clone http://gitlab-ci-token:${CI_JOB_TOKEN}@gitlab.local/root/repo.git
"#,
        )
        .unwrap();

        let report = doctor_repo(root).unwrap();
        let codes = report
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>();

        assert!(!report.ok);
        assert!(codes.contains(&"github_workflow_direct_push"));
        assert!(codes.contains(&"raw_local_gitlab_api"));
        assert!(codes.contains(&"local_gitlab_token_bypass"));
        assert!(codes.contains(&"token_bearing_git_remote"));
    }

    #[test]
    fn detects_fail_open_critical_gitlab_lanes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join(".jeryu")).unwrap();
        fs::write(
            root.join(".jeryu/ci.toml"),
            r#"schema_version = "1"
local_gitlab_required = true

[runner]
policy = "untagged-only"
forbid_tags = true
standard_jobs_untagged = true

[image]
rust = "rust:1.95.0"
pinned = true
"#,
        )
        .unwrap();
        fs::write(
            root.join(".gitlab-ci.yml"),
            r#"gitlab_source_auth_doctor:
  allow_failure: true
  script: bash scripts/ci/ci-job-token-clone-doctor.sh
jankurai_proof:
  allow_failure: "true"
  script: bash ops/ci/jankurai-lane.sh proof
"#,
        )
        .unwrap();

        let report = doctor_repo(root).unwrap();

        assert!(!report.ok);
        assert_eq!(
            report
                .findings
                .iter()
                .filter(|finding| finding.code == "critical_ci_lane_fail_open")
                .count(),
            2
        );
    }
}
