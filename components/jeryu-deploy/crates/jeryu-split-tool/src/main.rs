//! Rust-native transition tooling for the Jeryu split-family manifest.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use toml::Value;

mod audit_census;
mod audit_evidence;
mod audit_readme;
mod audit_scheduler;
mod audit_score;
mod build_config;
mod canonical_json;
mod mirror_update;
mod monorepo;
mod proof_inventory;
mod split_export;
mod split_tree;

#[derive(Debug, Parser)]
#[command(name = "jeryu-split")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check a managed README link block; update it only with --write.
    AuditReadme {
        #[arg(long)]
        readme: PathBuf,
        #[arg(long)]
        image_url: String,
        #[arg(long)]
        report_url: String,
        #[arg(long)]
        write: bool,
    },
    /// Enumerate every newly reachable source commit without publishing results.
    AuditPlan {
        #[arg(long)]
        source_repo: PathBuf,
        #[arg(long)]
        request: PathBuf,
    },
    /// Execute a complete audit census; unavailable sources remain explicit failures.
    AuditCensus {
        #[arg(long, default_value = "agent/audit-repositories.json")]
        inventory: PathBuf,
        /// New owner-only output directory; existing attempts are never replaced.
        #[arg(long)]
        out: PathBuf,
        /// Executable held and verified by the public auditor bootstrap.
        #[arg(long)]
        auditor: Option<PathBuf>,
        #[arg(long)]
        receipt: Option<PathBuf>,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
        /// Policy comparison commit; does not authenticate protected history.
        #[arg(long)]
        governing_commit: Option<String>,
    },
    /// Admit an existing auditor report using its owning score policy.
    AuditScoreCheck {
        #[arg(long, value_enum)]
        owner: audit_score::Owner,
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Validate one local identity for every Jeryu package in the monorepo.
    MonorepoCheck,
    /// Check component compiler/config projections; explicitly refresh with --write.
    BuildConfig {
        #[arg(long)]
        write: bool,
    },
    /// Fail unless every external Git dependency uses a public immutable source.
    PublicPreflight,
    /// Inventory all retained workflows, proof declarations and CI entrypoints.
    ProofInventory {
        #[arg(long)]
        check: bool,
    },
    /// Preview a deterministic standalone component tree without publishing refs.
    ExportTree {
        #[arg(long)]
        component: String,
        #[arg(long)]
        source: String,
        /// Regenerate and validate the standalone lock in a disposable Git clone.
        #[arg(long)]
        resolve_lock: bool,
        /// Use an explicitly verified local source for preparatory lock resolution.
        #[arg(long, requires = "resolve_lock")]
        prepare_local: Option<PathBuf>,
    },
    /// Prepare an offline successor to an existing mirror without updating refs.
    PrepareMirrorUpdate {
        #[arg(long)]
        source_repo: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        component: String,
        #[arg(long)]
        export_tree: String,
        #[arg(long)]
        mirror: PathBuf,
        #[arg(long)]
        expected_tip: String,
        /// Exact for-each-ref tag snapshot from the admitted existing mirror.
        #[arg(long)]
        expected_tags: PathBuf,
        /// Explicit first transition of an existing mirror without split provenance.
        #[arg(long)]
        initial_tip: Option<String>,
    },
    /// Validate and render the split-family manifest.
    Manifest {
        #[arg(long, default_value = "repos.manifest.toml")]
        manifest: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        check_paths: bool,
    },
    /// Prove that every source-tree path is assigned to a split repository.
    SourceCoverage {
        #[arg(long, default_value = "repos.manifest.toml")]
        manifest: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Run the governed score or full check lane for every manifest repository.
    FleetCi {
        #[arg(long, default_value = "repos.manifest.toml")]
        manifest: PathBuf,
        #[arg(long)]
        full: bool,
    },
    /// Validate the split release lock.
    VerifyLock {
        #[arg(long, default_value = "jeryu-split.lock.toml")]
        lock: PathBuf,
    },
    /// Run manifest, source-coverage, and lock checks without Python.
    ProductPipeline {
        #[arg(long, default_value = "repos.manifest.toml")]
        manifest: PathBuf,
        #[arg(long, default_value = "jeryu-split.lock.toml")]
        lock: PathBuf,
    },
    /// Verify hosted workflows remain thin wrappers around agent/ci-lanes.toml.
    CiLanesCheck,
    /// List commands declared by agent/ci-lanes.toml.
    CiLanesList {
        /// Emit only lanes that participate in the full workflow union.
        #[arg(long)]
        full: bool,
        /// Emit JSON instead of tab-separated lane/command rows.
        #[arg(long)]
        json: bool,
    },
    /// Build the affected-package plan for a committed change range.
    AffectedPlan {
        /// Protected base ref used for the three-dot diff.
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Output path relative to the repository root.
        #[arg(long, default_value = "target/ci-fast/affected-plan.json")]
        out: PathBuf,
        /// Worker count recorded in the plan.
        #[arg(long, default_value_t = 40)]
        workers: u32,
    },
}

#[derive(Debug, Serialize)]
struct SourceCoverageReport {
    missing: Vec<String>,
    missing_count: usize,
    patterns: usize,
    schema_version: &'static str,
    source_git_dir: Option<String>,
    source_reader: String,
    source_root: String,
    source_sha: String,
    status: &'static str,
    tracked_files: usize,
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    let cli = if args.get(1).and_then(|arg| arg.to_str()) == Some("manifest") {
        match parse_manifest_compat(&args) {
            Ok(cli) => cli,
            Err(error) => {
                eprint!("{}", error.stderr);
                return ExitCode::from(error.exit_code);
            }
        }
    } else {
        Cli::parse_from(args)
    };
    if let Command::Manifest { manifest, .. } = &cli.command
        && fs::File::open(manifest).is_err()
    {
        eprintln!(
            "manifest error: manifest not readable: {}",
            manifest.display()
        );
        return ExitCode::from(1);
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ManifestCliError {
    exit_code: u8,
    stderr: String,
}

fn parse_manifest_compat(args: &[OsString]) -> std::result::Result<Cli, ManifestCliError> {
    let program = env::var_os("JERYU_SPLIT_MANIFEST_PROGRAM")
        .filter(|value| !value.is_empty())
        .or_else(|| args.first().cloned())
        .unwrap_or_else(|| OsString::from("jeryu-split"));
    let program = program.to_string_lossy();
    let program =
        if program.is_empty() || program.len() > 4096 || program.chars().any(char::is_control) {
            "jeryu-split"
        } else {
            &program
        };
    let mut manifest = PathBuf::from("repos.manifest.toml");
    let mut json = false;
    let mut check_paths = false;
    let mut index = 2;
    while index < args.len() {
        match args[index].to_str() {
            Some("--manifest") => {
                index += 1;
                let Some(path) = args.get(index) else {
                    return Err(ManifestCliError {
                        exit_code: 1,
                        stderr: String::new(),
                    });
                };
                manifest = PathBuf::from(path);
            }
            Some("--json") => json = true,
            Some("--check-paths") => check_paths = true,
            _ => {
                return Err(ManifestCliError {
                    exit_code: 2,
                    stderr: format!(
                        "usage: {program} [--manifest PATH] [--json] [--check-paths]\n"
                    ),
                });
            }
        }
        index += 1;
    }
    if manifest.as_os_str().is_empty() {
        return Err(ManifestCliError {
            exit_code: 1,
            stderr: "manifest error: --manifest requires a path\n".to_owned(),
        });
    }
    Ok(Cli {
        command: Command::Manifest {
            manifest,
            json,
            check_paths,
        },
    })
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::AuditReadme {
            readme,
            image_url,
            report_url,
            write,
        } => audit_readme::run(&readme, &image_url, &report_url, write),
        Command::AuditPlan {
            source_repo,
            request,
        } => audit_scheduler::run(&source_repo, &request),
        Command::AuditCensus {
            inventory,
            out,
            auditor,
            receipt,
            timeout_seconds,
            governing_commit,
        } => audit_census::run(
            Path::new("."),
            &inventory,
            &out,
            auditor.as_deref(),
            receipt.as_deref(),
            timeout_seconds,
            governing_commit.as_deref(),
        ),
        Command::AuditScoreCheck {
            owner,
            policy,
            report,
        } => audit_score::run(owner, &policy, &report),
        Command::MonorepoCheck => monorepo::check(Path::new(".")),
        Command::BuildConfig { write } => build_config::run(Path::new("."), write),
        Command::PublicPreflight => monorepo::public_preflight(Path::new(".")),
        Command::ProofInventory { check } => proof_inventory::run(Path::new("."), check),
        Command::ExportTree {
            component,
            source,
            resolve_lock,
            prepare_local,
        } => split_tree::export_tree(
            Path::new("."),
            &component,
            &source,
            resolve_lock,
            prepare_local.as_deref(),
        ),
        Command::PrepareMirrorUpdate {
            source_repo,
            source,
            component,
            export_tree,
            mirror,
            expected_tip,
            expected_tags,
            initial_tip,
        } => mirror_update::prepare(mirror_update::Request {
            source_repo: &source_repo,
            source: &source,
            component: &component,
            export_tree: &export_tree,
            mirror: &mirror,
            expected_tip: &expected_tip,
            expected_tags: &expected_tags,
            initial_tip: initial_tip.as_deref(),
        }),
        Command::Manifest {
            manifest,
            json,
            check_paths,
        } => manifest_command(&manifest, json, check_paths),
        Command::SourceCoverage { manifest, json } => source_coverage(&manifest, json),
        Command::FleetCi { manifest, full } => fleet_ci(&manifest, full),
        Command::VerifyLock { lock } => verify_lock(&lock),
        Command::ProductPipeline { manifest, lock } => product_pipeline(&manifest, &lock),
        Command::CiLanesCheck => {
            emit_repo_gate(jeryu_repogate::run_ci_lanes_check(Path::new("."))?)
        }
        Command::CiLanesList { full, json } => emit_repo_gate(jeryu_repogate::run_ci_lanes_list(
            Path::new("."),
            full,
            json,
        )?),
        Command::AffectedPlan { base, out, workers } => emit_repo_gate(
            jeryu_repogate::run_affected_plan(Path::new("."), &base, &out, workers)?,
        ),
    }
}

fn emit_repo_gate(outcome: jeryu_repogate::GateOutcome) -> Result<()> {
    for line in outcome.stdout {
        println!("{line}");
    }
    if outcome.exit_code != 0 {
        bail!("repository gate exited {}", outcome.exit_code);
    }
    Ok(())
}

fn read_toml(path: &Path) -> Result<Value> {
    let source =
        fs::read_to_string(path).with_context(|| format!("read TOML from {}", path.display()))?;
    toml::from_str(&source).with_context(|| format!("parse TOML from {}", path.display()))
}

fn table<'a>(value: &'a Value, context: &str) -> Result<&'a toml::Table> {
    value
        .as_table()
        .with_context(|| format!("{context} must be a TOML table"))
}

fn required_string<'a>(table: &'a toml::Table, field: &str, context: &str) -> Result<&'a str> {
    let value = table
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if value.is_empty() {
        bail!("{context} missing {field}");
    }
    Ok(value)
}

fn string_array(table: &toml::Table, field: &str, context: &str) -> Result<Vec<String>> {
    let Some(value) = table.get(field) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .with_context(|| format!("{context}.{field} must be an array"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("{context}.{field} entries must be strings"))
        })
        .collect()
}

fn repositories(value: &Value) -> Result<&Vec<Value>> {
    table(value, "manifest")?
        .get("repo")
        .and_then(Value::as_array)
        .filter(|repos| !repos.is_empty())
        .context("manifest must contain [[repo]] entries")
}

fn validate_manifest_value(value: &Value, check_paths: bool) -> Result<()> {
    let root = table(value, "manifest")?;
    if root.get("schema_version").and_then(Value::as_str) == Some("jeryu.monorepo/v1") {
        return monorepo::validate_manifest(value, check_paths);
    }
    let repos = repositories(value)?;
    let mut seen = BTreeSet::new();

    for repo in repos {
        let repo = table(repo, "repo entry")?;
        let name = required_string(repo, "name", "repo entry")?;
        if !seen.insert(name.to_owned()) {
            bail!("duplicate repo name: {name}");
        }
        let path = required_string(repo, "path", name)?;
        for field in [
            "github_slug",
            "jeryu_slug",
            "profile",
            "default_branch",
            "current_tag",
            "required_check",
        ] {
            required_string(repo, field, name)?;
        }
        if required_string(repo, "default_branch", name)? != "main" {
            bail!("{name} default_branch must be main");
        }
        if repo.get("has_jeryu_std").and_then(Value::as_bool) != Some(true) {
            bail!("{name} must set has_jeryu_std=true");
        }
        if check_paths {
            let repo_path = Path::new(path);
            if !repo_path.is_dir() {
                bail!("{name} path missing: {path}");
            }
            for required in ["AGENTS.md", "agent/owner-map.json", "agent/test-map.json"] {
                if !repo_path.join(required).is_file() {
                    bail!("{name} missing {required}");
                }
            }
        }
    }

    let required = string_array(root, "required_repos", "manifest")?;
    let missing: Vec<_> = required
        .into_iter()
        .filter(|name| !seen.contains(name))
        .collect();
    if !missing.is_empty() {
        bail!("manifest missing required repos: {}", missing.join(" "));
    }
    Ok(())
}

fn manifest_command(path: &Path, json: bool, check_paths: bool) -> Result<()> {
    let value = read_toml(path)?;
    let required = string_array(table(&value, "manifest")?, "required_repos", "manifest")?;
    if !required.iter().any(|name| name == "jeryu") {
        bail!("manifest.required_repos must include the public portal repo jeryu");
    }
    validate_manifest_value(&value, check_paths)?;
    let repos = repositories(&value)?;
    if json {
        let output = serde_json::json!({ "repo": repos });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }
    for repo in repos {
        let repo = table(repo, "repo entry")?;
        println!(
            "{}|{}|{}|{}",
            required_string(repo, "name", "repo entry")?,
            required_string(repo, "path", "repo entry")?,
            required_string(repo, "github_slug", "repo entry")?,
            required_string(repo, "jeryu_slug", "repo entry")?,
        );
    }
    Ok(())
}

fn source_coverage(path: &Path, json: bool) -> Result<()> {
    let value = read_toml(path)?;
    let root = table(&value, "manifest")?;
    let source_root = PathBuf::from(required_string(root, "source_root", "manifest")?);
    let source_sha = required_string(root, "source_sha", "manifest")?.to_owned();
    let source_git_dir = root
        .get("source_git_dir")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut patterns = string_array(root, "shared_source_paths", "manifest")?;
    for repo in repositories(&value)? {
        patterns.extend(string_array(
            table(repo, "repo entry")?,
            "source_paths",
            "repo entry",
        )?);
    }
    let (files, source_reader) = git_tree(&source_root, source_git_dir.as_deref(), &source_sha)?;
    let missing: Vec<_> = files
        .iter()
        .filter(|path| !is_covered(path, &patterns))
        .cloned()
        .collect();
    let status = if missing.is_empty() { "pass" } else { "fail" };
    let report = SourceCoverageReport {
        missing_count: missing.len(),
        missing,
        patterns: patterns.len(),
        schema_version: "jeryu.split.source-coverage/v1",
        source_git_dir: source_git_dir
            .as_ref()
            .map(|path| path.display().to_string()),
        source_reader,
        source_root: source_root.display().to_string(),
        source_sha,
        status,
        tracked_files: files.len(),
    };
    if json {
        println!("{}", source_coverage_json(&report)?);
    } else if report.missing.is_empty() {
        println!(
            "source coverage pass: {} tracked files covered by {} patterns",
            report.tracked_files, report.patterns
        );
    } else {
        println!(
            "source coverage failed: {} tracked files are not assigned",
            report.missing_count
        );
        for path in report.missing.iter().take(100) {
            println!("{path}");
        }
    }
    if report.missing.is_empty() {
        Ok(())
    } else {
        bail!("source coverage failed")
    }
}

fn source_coverage_json(report: &SourceCoverageReport) -> Result<String> {
    Ok(serde_json::to_string_pretty(report)?)
}

fn git_tree(
    source_root: &Path,
    source_git_dir: Option<&Path>,
    source_sha: &str,
) -> Result<(Vec<String>, String)> {
    let (mut command, reader) = if source_root.exists() {
        let mut command = ProcessCommand::new("git");
        command.args(["-C", &source_root.display().to_string()]);
        (command, source_root.display().to_string())
    } else if let Some(git_dir) = source_git_dir {
        let mut command = ProcessCommand::new("git");
        command.arg(format!("--git-dir={}", git_dir.display()));
        (command, git_dir.display().to_string())
    } else {
        let mut command = ProcessCommand::new("git");
        command.args(["-C", &source_root.display().to_string()]);
        (command, format!("{} (missing)", source_root.display()))
    };
    let output = command
        .args(["ls-tree", "-r", "--name-only", source_sha])
        .output()
        .with_context(|| format!("read source tree from {reader}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        });
        bail!(
            "failed to read source tree from {reader}: {}",
            detail.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("source tree paths must be UTF-8")?;
    Ok((
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_owned)
            .collect(),
        reader,
    ))
}

fn is_covered(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        path == pattern
            || pattern
                .strip_suffix("/**")
                .is_some_and(|prefix| path.starts_with(&format!("{prefix}/")))
            || wildcard_matches(pattern, path)
    })
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern: Vec<_> = pattern.chars().collect();
    let value: Vec<_> = value.chars().collect();
    let mut memo = vec![vec![None; value.len() + 1]; pattern.len() + 1];
    wildcard_matches_at(&pattern, &value, 0, 0, &mut memo)
}

fn wildcard_matches_at(
    pattern: &[char],
    value: &[char],
    pattern_index: usize,
    value_index: usize,
    memo: &mut [Vec<Option<bool>>],
) -> bool {
    if let Some(result) = memo[pattern_index][value_index] {
        return result;
    }
    let result = if pattern_index == pattern.len() {
        value_index == value.len()
    } else {
        match pattern[pattern_index] {
            '*' => {
                wildcard_matches_at(pattern, value, pattern_index + 1, value_index, memo)
                    || (value_index < value.len()
                        && wildcard_matches_at(
                            pattern,
                            value,
                            pattern_index,
                            value_index + 1,
                            memo,
                        ))
            }
            '?' if value_index < value.len() => {
                wildcard_matches_at(pattern, value, pattern_index + 1, value_index + 1, memo)
            }
            '[' if value_index < value.len() => {
                if let Some((next_index, class_matches)) =
                    character_class(pattern, pattern_index, value[value_index])
                {
                    class_matches
                        && wildcard_matches_at(pattern, value, next_index, value_index + 1, memo)
                } else {
                    value[value_index] == '['
                        && wildcard_matches_at(
                            pattern,
                            value,
                            pattern_index + 1,
                            value_index + 1,
                            memo,
                        )
                }
            }
            token if value_index < value.len() && token == value[value_index] => {
                wildcard_matches_at(pattern, value, pattern_index + 1, value_index + 1, memo)
            }
            _ => false,
        }
    };
    memo[pattern_index][value_index] = Some(result);
    result
}

fn character_class(pattern: &[char], open_index: usize, value: char) -> Option<(usize, bool)> {
    let mut index = open_index + 1;
    let negated = pattern.get(index) == Some(&'!');
    if negated {
        index += 1;
    }
    let start = index;
    let mut matched = false;
    while index < pattern.len() && (pattern[index] != ']' || index == start) {
        let first = pattern[index];
        if pattern.get(index + 1) == Some(&'-')
            && pattern.get(index + 2).is_some_and(|token| *token != ']')
        {
            let last = pattern[index + 2];
            matched |= first <= value && value <= last;
            index += 3;
        } else {
            matched |= first == value;
            index += 1;
        }
    }
    (index < pattern.len() && pattern[index] == ']').then_some((index + 1, matched != negated))
}

fn fleet_ci(path: &Path, full: bool) -> Result<()> {
    let value = read_toml(path)?;
    for (name, path, lane) in fleet_entries(&value, full)? {
        println!("{name}: just {lane}");
        io::stdout().flush()?;
        run_process(
            ProcessCommand::new("just").arg(&lane).current_dir(path),
            &name,
        )?;
    }
    Ok(())
}

fn fleet_entries(value: &Value, full: bool) -> Result<Vec<(String, PathBuf, String)>> {
    validate_manifest_value(value, false)?;
    let lane = if full { "check" } else { "score" };
    repositories(value)?
        .iter()
        .map(|repo| {
            let repo = table(repo, "repo entry")?;
            let name = required_string(repo, "name", "repo entry")?.to_owned();
            let path = PathBuf::from(required_string(repo, "path", &name)?);
            Ok((name, path, lane.to_owned()))
        })
        .collect()
}

fn verify_lock(path: &Path) -> Result<()> {
    let value = read_toml(path)?;
    verify_lock_value(&value)?;
    println!("lock ok: {} repos", repositories(&value)?.len());
    Ok(())
}

fn verify_lock_value(value: &Value) -> Result<()> {
    let repos = repositories(value)?;
    let mut failures = Vec::new();
    for repo in repos {
        let repo = table(repo, "lock repo entry")?;
        let name = repo
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("<unknown>");
        for field in [
            "name",
            "github_slug",
            "local_path",
            "tag",
            "commit",
            "required_check",
        ] {
            if repo
                .get(field)
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                failures.push(format!("{name} missing {field}"));
            }
        }
        let commit = repo
            .get("commit")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let valid_sha = commit.len() == 40
            && commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if commit != "PENDING" && commit != "PENDING_SELF" && !valid_sha {
            failures.push(format!("{name} commit is not a sha: {commit}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        bail!(failures.join("\n"))
    }
}

fn product_pipeline(manifest: &Path, lock: &Path) -> Result<()> {
    println!("+ jeryu-split manifest --check-paths");
    manifest_command(manifest, false, true)?;
    println!("+ jeryu-split source-coverage");
    source_coverage(manifest, false)?;
    println!("+ jeryu-split verify-lock");
    verify_lock(lock)?;
    println!("product pipeline bootstrap ok");
    Ok(())
}

fn run_process(command: &mut ProcessCommand, context: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("launch governed command for {context}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("governed command for {context} failed with {status}")
    }
}

#[cfg(test)]
mod tests;
