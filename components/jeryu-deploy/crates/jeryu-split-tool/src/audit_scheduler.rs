//! Read-only, complete Git-graph planning. This is not an executor or publisher.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};

#[path = "audit_scheduler_process.rs"]
mod process;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Event {
    Push,
    PullRequest,
    Release,
    Reconcile,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutionIdentity {
    pub(super) repository: String,
    pub(super) scope: String,
    pub(super) auditor_source_commit: String,
    auditor_executable_sha256: String,
    auditor_receipt_sha256: String,
    /// The accepted governing policy and its source-policy resolution rules.
    /// Candidate policy bytes are independently bound by each source tree.
    pub(super) governing_policy_sha256: String,
    /// Includes all executor/toolchain/command/dependency-lock selection inputs.
    pub(super) execution_config_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) schema_version: String,
    pub(super) identity: ExecutionIdentity,
    pub(super) event: Event,
    pub(super) source_ref: String,
    /// Push's previous full object ID, or null / all-zero ID for creation.
    pub(super) before: Option<String>,
    /// Full commit ID; release also accepts its full annotated-tag object ID.
    pub(super) after: Option<String>,
    pub(super) run_id: String,
    pub(super) run_attempt: u32,
    /// Preserve a retry relationship without changing the source deduplication key.
    pub(super) previous_run_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Job {
    pub(super) source_commit: String,
    pub(super) source_tree: String,
    pub(super) deduplication_key: String,
    pub(super) attempt_key: String,
    pub(super) status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    pub(super) schema_version: String,
    pub(super) identity: ExecutionIdentity,
    pub(super) event: Event,
    pub(super) source_ref: String,
    pub(super) before: Option<String>,
    pub(super) after: Option<String>,
    pub(super) resolved_after_commit: Option<String>,
    pub(super) disposition: String,
    pub(super) run_id: String,
    pub(super) run_attempt: u32,
    pub(super) previous_run_id: Option<String>,
    /// These commits keep any existing evidence; this plan never removes it.
    pub(super) withdrawn_commits: Vec<String>,
    pub(super) jobs: Vec<Job>,
    pub(super) execution_verified: bool,
    pub(super) publication_qualified: bool,
}

fn git_command(repo: &Path) -> Command {
    let mut command = crate::split_tree::source_git_command(repo);
    command.env("GIT_NO_LAZY_FETCH", "1").args([
        "-c",
        "protocol.allow=never",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "log.showSignature=false",
        "-c",
        "core.commitGraph=false",
    ]);
    command
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = process::git_output(git_command(repo).args(args))?;
    ensure!(
        output.status.success() && output.stderr.is_empty(),
        "audit source graph unavailable: Git {} failed; supply complete local objects",
        args[0]
    );
    String::from_utf8(output.stdout).context("non-UTF-8 Git graph output")
}

pub(super) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
}

fn validate(request: &Request) -> Result<()> {
    ensure!(
        request.schema_version == "jeryu.audit-plan-request/v1",
        "unsupported audit-plan request"
    );
    let identity = &request.identity;
    let parts: Vec<_> = identity.repository.split('/').collect();
    ensure!(
        parts.len() == 2 && parts.iter().all(|part| identifier(part)),
        "repository must be owner/name"
    );
    ensure!(identifier(&identity.scope), "invalid audit scope");
    ensure!(
        hex(&identity.auditor_source_commit, 40),
        "invalid auditor source commit"
    );
    for digest in [
        &identity.auditor_executable_sha256,
        &identity.auditor_receipt_sha256,
        &identity.governing_policy_sha256,
        &identity.execution_config_sha256,
    ] {
        ensure!(hex(digest, 64), "invalid execution identity SHA-256");
    }
    for oid in [&request.before, &request.after].into_iter().flatten() {
        ensure!(
            hex(oid, 40),
            "event requires lowercase full SHA-1 object IDs"
        );
    }
    ensure!(
        identifier(&request.run_id) && request.run_attempt > 0,
        "invalid run identity"
    );
    if let Some(previous) = &request.previous_run_id {
        ensure!(identifier(previous), "invalid previous run identity");
    }
    ensure!(identifier(&request.source_ref), "invalid source ref");
    let prefix_ok = match request.event {
        Event::Push | Event::Reconcile => request.source_ref.starts_with("refs/heads/"),
        Event::PullRequest => {
            let parts: Vec<_> = request.source_ref.split('/').collect();
            parts.len() == 4
                && parts[0] == "refs"
                && parts[1] == "pull"
                && parts[2].parse::<u64>().is_ok_and(|number| number > 0)
                && matches!(parts[3], "head" | "merge")
        }
        Event::Release => request.source_ref.starts_with("refs/tags/"),
    };
    ensure!(prefix_ok, "source ref does not match event kind");
    Ok(())
}

fn nonzero(value: Option<&str>) -> Option<&str> {
    value.filter(|value| value.bytes().any(|b| b != b'0'))
}

fn admit_graph(repo: &Path) -> Result<()> {
    ensure!(
        repo.is_absolute() && repo.canonicalize()? == repo,
        "source repo must be a physical absolute path"
    );
    ensure!(
        git(repo, &["rev-parse", "--show-object-format"])?.trim() == "sha1",
        "unsupported source object format"
    );
    ensure!(
        git(repo, &["rev-parse", "--is-shallow-repository"])?.trim() == "false",
        "shallow source graph: fetch complete history before planning"
    );
    let storage = git(repo, &["rev-parse", "--absolute-git-dir"])?;
    let storage = Path::new(storage.trim());
    ensure!(
        storage.canonicalize()? == storage,
        "source Git storage must be physical"
    );
    for name in ["info/grafts", "objects/info/alternates", "commondir"] {
        match fs::symlink_metadata(storage.join(name)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => bail!("external or rewritten source graph is unsupported: {name}"),
        }
    }
    ensure!(
        git(
            repo,
            &["for-each-ref", "--format=%(refname)", "refs/replace/"]
        )?
        .is_empty(),
        "replacement source refs are unsupported"
    );
    Ok(())
}

fn commit(repo: &Path, oid: &str, tag_allowed: bool) -> Result<String> {
    let kind = git(repo, &["cat-file", "-t", oid])?;
    ensure!(
        kind.trim() == "commit" || (tag_allowed && kind.trim() == "tag"),
        "event object is not a commit or admitted release tag"
    );
    let resolved = git(
        repo,
        &["rev-parse", "--verify", &format!("{oid}^{{commit}}")],
    )?;
    let resolved = resolved.trim();
    ensure!(hex(resolved, 40), "invalid resolved commit");
    Ok(resolved.to_owned())
}

pub(super) fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

fn graph_rows(repo: &Path, after: &str, before: Option<&str>) -> Result<Vec<(String, String)>> {
    let excluded = before.map(|before| format!("^{before}"));
    let mut args = vec![
        "rev-list",
        "--reverse",
        "--topo-order",
        "--no-commit-header",
        "--format=%H %T",
        after,
    ];
    if let Some(excluded) = &excluded {
        args.push(excluded);
    }
    args.push("--");
    git(repo, &args)?
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split(' ').collect();
            ensure!(
                fields.len() == 2 && fields.iter().all(|field| hex(field, 40)),
                "malformed Git graph row"
            );
            Ok((fields[0].to_owned(), fields[1].to_owned()))
        })
        .collect()
}

pub(super) fn plan(repo: &Path, request: Request) -> Result<Plan> {
    validate(&request)?;
    admit_graph(repo)?;
    git(repo, &["check-ref-format", &request.source_ref])?;
    let before = nonzero(request.before.as_deref())
        .map(|value| commit(repo, value, false))
        .transpose()?;
    let after = nonzero(request.after.as_deref())
        .map(|value| commit(repo, value, request.event == Event::Release))
        .transpose()?;
    ensure!(
        before.is_some() || after.is_some(),
        "event has neither previous nor current source"
    );
    ensure!(
        request.event == Event::Push || (before.is_none() && after.is_some()),
        "non-push events require only an after object"
    );
    let mut rows = Vec::new();
    let mut withdrawn = Vec::new();
    let disposition;
    if request.source_ref == "refs/heads/audit-evidence" {
        disposition = "excluded_evidence_branch";
    } else if let Some(after) = &after {
        if matches!(request.event, Event::PullRequest | Event::Release) {
            // A PR or release may be the first enrolled reference to these
            // commits. Include both merge parents and all ancestors; accepted
            // identical execution identities are deduplicated by the ledger.
            rows = graph_rows(repo, after, None)?;
            disposition = "exact_revision";
        } else {
            rows = graph_rows(repo, after, before.as_deref())?;
            if let Some(before) = &before {
                withdrawn = graph_rows(repo, before, Some(after))?
                    .into_iter()
                    .map(|(commit, _)| commit)
                    .collect();
                disposition = if !withdrawn.is_empty() {
                    "rewritten"
                } else if rows.is_empty() {
                    "unchanged"
                } else {
                    "advanced"
                };
            } else {
                // Full ancestry is intentional: never trust a truncated webhook array.
                // Reconciliation deduplicates only against identical accepted identities.
                disposition = if request.event == Event::Reconcile {
                    "reconciled"
                } else {
                    "created"
                };
            }
        }
    } else {
        disposition = "deleted";
    }
    let jobs = rows
        .into_iter()
        .map(|(source_commit, source_tree)| {
            let deduplication_key = source_key(&request.identity, &source_commit, &source_tree)?;
            let attempt_key =
                attempt_key(&deduplication_key, &request.run_id, request.run_attempt)?;
            Ok(Job {
                source_commit,
                source_tree,
                deduplication_key,
                attempt_key,
                status: "pending".into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Plan {
        schema_version: "jeryu.audit-plan/v1".into(),
        identity: request.identity,
        event: request.event,
        source_ref: request.source_ref,
        before: request.before,
        after: request.after,
        resolved_after_commit: after,
        disposition: disposition.into(),
        run_id: request.run_id,
        run_attempt: request.run_attempt,
        previous_run_id: request.previous_run_id,
        withdrawn_commits: withdrawn,
        jobs,
        execution_verified: false,
        publication_qualified: false,
    })
}

/// These keys are shared with the local ledger. Deserialized keys are never authority.
pub(super) fn source_key(identity: &ExecutionIdentity, commit: &str, tree: &str) -> Result<String> {
    digest(&("jeryu.audit-source-key/v1", identity, commit, tree))
}

pub(super) fn attempt_key(source_key: &str, run_id: &str, run_attempt: u32) -> Result<String> {
    digest(&(
        "jeryu.audit-attempt-key/v1",
        source_key,
        run_id,
        run_attempt,
    ))
}

#[path = "audit_scheduler_input.rs"]
mod input;
pub(super) use input::{import_plan, plan_event_key};

pub(super) fn run(source_repo: &Path, request_file: &Path) -> Result<()> {
    let bytes = fs::read(request_file).context("read audit-plan request")?;
    ensure!(
        bytes
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(b'{'),
        "audit-plan request must be a JSON object"
    );
    let request = serde_json::from_slice(&bytes).context("invalid audit-plan request JSON")?;
    let report = plan(source_repo, request)?;
    println!(
        "{}",
        crate::canonical_json::pretty(serde_json::to_value(report)?)?
    );
    Ok(())
}
