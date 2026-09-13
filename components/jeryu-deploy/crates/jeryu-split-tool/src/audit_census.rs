//! Execute every enrolled scope and retain failed attempts as first-class results.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::{fd::AsRawFd, unix::fs::DirBuilderExt},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::audit_evidence::{self, Binding, Summary, hash};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub repository: String,
    pub scope: String,
    pub path: Option<String>,
    pub commit: Option<String>,
    pub minimum: u8,
    pub required: bool,
    pub reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    schema: String,
    sources: Vec<Source>,
}

#[derive(Serialize)]
struct Row {
    source: Source,
    acquisition: Option<Value>,
    status: &'static str,
    reason: String,
    commit: Option<String>,
    tree: Option<String>,
    policy_sha256: Option<String>,
    governing_policy_sha256: Option<String>,
    command_exit: Option<i32>,
    report_sha256: Option<String>,
    summary: Option<Summary>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = crate::split_tree::source_git_command(root)
        .env("GIT_NO_LAZY_FETCH", "1")
        .args(args)
        .output()?;
    ensure!(
        out.status.success(),
        "source Git operation failed: {}",
        args[0]
    );
    Ok(String::from_utf8(out.stdout)?.trim_end().to_owned())
}

fn oid(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "full lowercase source commit required"
    );
    Ok(())
}

fn snapshot(root: &Path) -> Result<(String, String)> {
    ensure!(
        git(root, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty(),
        "audit requires clean committed source"
    );
    ensure!(!git(root, &["ls-files", "-v"])?.lines().any(|line| line.starts_with('S') || line.starts_with(|c: char| c.is_ascii_lowercase())), "hidden index entries are forbidden");
    Ok((
        git(root, &["rev-parse", "HEAD"])?,
        git(root, &["rev-parse", "HEAD^{tree}"])?,
    ))
}

pub(super) fn effective_floor(name: &str) -> u8 {
    if matches!(name, "jeryu-cache" | "jeryu-jira" | "jeryu-ci-runner") {
        91
    } else {
        85
    }
}

#[path = "audit_enrollment.rs"]
pub(super) mod enrollment;

fn sources(root: &Path, inventory: &Path) -> Result<Vec<Source>> {
    let manifest = toml::from_str(&fs::read_to_string(root.join("repos.manifest.toml"))?)?;
    let inventory = serde_json::from_slice(&fs::read(inventory)?)?;
    enrollment::sources(&manifest, inventory)
}

fn source_path(root: &Path, source: &Source) -> Result<PathBuf> {
    let relative = source.path.as_ref().context("source unavailable")?;
    ensure!(
        Path::new(relative)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "source path escapes project cache"
    );
    let supplied = if relative == "." {
        root.to_owned()
    } else {
        root.join(relative)
    };
    let path = supplied.canonicalize()?;
    ensure!(
        path == supplied && path.starts_with(root),
        "source must be physical within project-controlled storage"
    );
    if matches!(
        source.scope.as_str(),
        "dependency" | "standalone" | "optional"
    ) {
        ensure!(
            source.commit.is_some(),
            "external audit needs an exact immutable source commit"
        );
    }
    Ok(path)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(file, "{}", crate::canonical_json::pretty(value.clone())?)?;
    file.sync_all()?;
    Ok(())
}

#[path = "audit_input.rs"]
mod input;
use input::{hash_binary, hold_binary, read_report};

#[path = "audit_bootstrap.rs"]
mod bootstrap;
use bootstrap::{pin, verify_auditor};

// Parsing preserves the owning pin grammar; it grants no qualification.
pub(super) fn publication_pin(source: &str, name: &str) -> Result<String> {
    bootstrap::parse_pin(source, name)
}

#[path = "audit_execution.rs"]
mod execution;
use execution::{Executor, execute};

#[path = "audit_acquisition.rs"]
mod acquisition;

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    root: &Path,
    inventory: &Path,
    out: &Path,
    auditor: Option<&Path>,
    receipt: Option<&Path>,
    timeout_seconds: u64,
    governing: Option<&str>,
) -> Result<()> {
    ensure!(
        (1..=3600).contains(&timeout_seconds),
        "audit timeout must be 1..3600 seconds"
    );
    let root = root.canonicalize()?;
    if let Some(governing) = governing {
        oid(governing)?;
    }
    let sources = sources(&root, inventory)?;
    ensure!(out.is_absolute(), "audit output must be absolute");
    ensure!(
        !out.starts_with(&root),
        "audit output must be outside audited source"
    );
    ensure!(
        out.parent().context("output parent")?.canonicalize()?
            == out.parent().context("output parent")?,
        "audit output parent must be physical"
    );
    fs::DirBuilder::new()
        .mode(0o700)
        .create(out)
        .context("use a new audit attempt directory")?;
    let out = out.canonicalize()?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let source_identity = snapshot(&root);
    let binary_pin = pin(&root, "JANKURAI_BINARY_SHA256");
    let version_pin = pin(&root, "JANKURAI_SEMVER");
    let binary_hash = binary_pin.as_deref().unwrap_or("");
    let version = version_pin.as_deref().unwrap_or("");
    let receipt_hash = receipt
        .and_then(|path| read_report(path).ok())
        .map(|bytes| hash(&bytes));
    let held_binary = auditor.and_then(|path| hold_binary(path).ok());
    let held_path = held_binary.as_ref().map(|file| {
        PathBuf::from(format!(
            "/proc/{}/fd/{}",
            std::process::id(),
            file.as_raw_fd()
        ))
    });
    let admission = source_identity
        .as_ref()
        .map(|_| ())
        .map_err(|error| format!("{error:#}"))
        .and_then(|_| match (auditor, receipt_hash.as_ref()) {
            (Some(binary), Some(_)) => {
                let verified = (|| -> Result<()> {
                    ensure!(
                        binary_pin.is_ok() && version_pin.is_ok(),
                        "missing or malformed auditor pins"
                    );
                    let held = held_path
                        .as_ref()
                        .context("cannot hold auditor executable")?;
                    ensure!(
                        hash_binary(held)? == binary_hash,
                        "wrong pinned auditor executable"
                    );
                    verify_auditor(
                        &root,
                        binary,
                        receipt.context("auditor receipt")?,
                        &source_identity
                            .as_ref()
                            .map_err(|error| anyhow::anyhow!("{error:#}"))?
                            .0,
                        &out.join("auditor-verification.log"),
                    )?;
                    let receipt_path = receipt.context("auditor receipt")?;
                    let verified_hash = hash(&read_report(receipt_path)?);
                    ensure!(
                        Some(&verified_hash) == receipt_hash.as_ref()
                            && receipt_path.file_name().and_then(|name| name.to_str())
                                == Some(&format!("{verified_hash}.json")),
                        "verified receipt bytes changed or lack their content address"
                    );
                    Ok(())
                })();
                verified.map_err(|error| format!("{error:#}"))
            }
            _ => Err(
                "verified public auditor and receipt unavailable; run scripts/ci.sh auditor".into(),
            ),
        });
    let mut rows = Vec::new();
    let mut storage_errors = Vec::new();
    for (index, source) in sources.into_iter().enumerate() {
        let mut row = Row {
            source,
            acquisition: None,
            status: "not_executed",
            reason: String::new(),
            commit: None,
            tree: None,
            policy_sha256: None,
            governing_policy_sha256: None,
            command_exit: None,
            report_sha256: None,
            summary: None,
        };
        // A failed source/auditor bootstrap performs no network acquisition.
        // Every scope still receives its explicit not-executed reason.
        let acquired = if row.source.path.is_none() && admission.is_ok() {
            match acquisition::acquire(&root, &out, &row.source, index, timeout_seconds) {
                Ok(acquired) => {
                    row.commit = Some(acquired.commit().to_owned());
                    row.tree = Some(acquired.tree().to_owned());
                    row.acquisition = Some(acquired.receipt.clone());
                    Some(acquired)
                }
                Err(error) => {
                    row.status = if error.downcast_ref::<acquisition::TimedOut>().is_some() {
                        "timed_out"
                    } else {
                        "source_unavailable"
                    };
                    row.reason = format!("{error:#}");
                    None
                }
            }
        } else {
            None
        };
        if row.source.path.is_some() || acquired.is_some() || admission.is_err() {
            if let Err(reason) = &admission {
                row.reason.clone_from(reason);
            } else if let Some(auditor) = held_path.as_deref() {
                let executor = Executor {
                    root: &root,
                    out: &out,
                    auditor,
                    binary_hash,
                    version,
                    timeout_seconds,
                    governing,
                };
                let result = match &acquired {
                    Some(source) => {
                        execution::execute_at(&executor, &mut row, index, source.path())
                    }
                    None => execute(&executor, &mut row, index),
                };
                if let Err(error) = result {
                    row.status = "tool_error";
                    row.reason = format!("{error:#}");
                }
            }
        }
        if let Some(acquired) = acquired
            && let Err(error) = acquired.finish(row.status == "passed")
        {
            row.status = "tool_error";
            row.reason =
                format!("public source cleanup refused; retained for inspection: {error:#}");
        }
        println!(
            "{} {}: {}",
            row.source.repository, row.source.scope, row.status
        );
        if let Err(error) = write_json(
            &out.join(format!("{index:04}.json")),
            &serde_json::to_value(&row)?,
        ) {
            storage_errors.push(format!("scope {index}: {error:#}"));
        }
        rows.push(row);
    }
    let scores_passed = rows
        .iter()
        .all(|row| !row.source.required || row.status == "passed");
    // A local Git object is not authenticated protected predecessor evidence.
    // Keep this admission open until the reviewed hosted verifier is connected.
    let passed = false;
    let value = json!({"schema":"jeryu.audit-census/v1", "started_at":started,
        "source_commit":source_identity.as_ref().ok().map(|v| &v.0), "source_tree":source_identity.as_ref().ok().map(|v| &v.1),
        "auditor_source":pin(&root, "JANKURAI_REV").ok(), "auditor_sha256":binary_hash, "auditor_receipt_sha256":receipt_hash,
        "inventory_sha256":hash(&fs::read(inventory)?), "cargo_lock_sha256":hash(&fs::read(root.join("Cargo.lock"))?),
        "governing_commit":governing, "protected_predecessor_authenticated":false,
        "complete":true, "storage_errors":storage_errors, "scores_passed":scores_passed,
        "passed":passed, "release_qualified":false, "results":rows});
    write_json(&out.join("census.json"), &value)?;
    println!("Census: {}", out.join("census.json").display());
    ensure!(
        passed,
        "required audit census contains failed or unavailable results"
    );
    Ok(())
}

#[cfg(test)]
#[path = "audit_census_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "audit_execution_tests.rs"]
mod execution_tests;
