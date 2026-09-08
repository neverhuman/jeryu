//! Server-side hook implementations.

use crate::error::{GitdError, Result};
use crate::lfs::RAW_GIT_BLOB_LIMIT_BYTES;
use crate::object_fsck::ObjectFsck;
use crate::protection::{ProtectedRefRule, RefChange, RefOperation};
use crate::refs::{is_zero_oid, validate_ref_name};
use crate::repo::Repository;
use std::io::{self, Read};

/// The protected trunk ref. Wire clients must use PRs; trusted server-side
/// merge/update code may still advance this ref through [`crate::refs::RefService`].
const MAIN_REF: &str = "refs/heads/main";

/// Template installed as `hooks/pre-receive` for bare repos served by Jeryu.
///
/// The shell layer rejects direct main pushes before invoking the Rust guard so
/// the PR-only policy remains durable even when Git itself launches the hook.
pub const PRE_RECEIVE_HOOK: &str = r#"#!/usr/bin/env bash
set -euo pipefail

input=$(cat)
if printf '%s\n' "$input" | awk '$3 == "refs/heads/main" { found = 1 } END { exit found ? 0 : 1 }'; then
  printf '%s\n' "jeryu-gitd: direct pushes to refs/heads/main are blocked; open a pull request and merge through Jeryu" >&2
  exit 1
fi

# Tags are write-once even when the optional jeryu-gitd helper is not on the
# git-http-backend PATH. Creation has an all-zero old oid; every update or
# deletion has a non-zero old oid and must fail closed.
if printf '%s\n' "$input" | awk '
  $3 ~ /^refs\/tags\// && $1 != "0000000000000000000000000000000000000000" { found = 1 }
  END { exit found ? 0 : 1 }
'; then
  printf '%s\n' "jeryu-gitd: immutable tags cannot be updated or deleted" >&2
  exit 1
fi

bin=${JERYU_GITD_BIN:-jeryu-gitd}
if command -v "$bin" >/dev/null 2>&1; then
  repo_dir=$(pwd -P)
  repo_git=${repo_dir##*/}
  owner_dir=${repo_dir%/*}
  owner=${owner_dir##*/}
  root=${owner_dir%/*}
  repo=${repo_git%.git}
  printf '%s\n' "$input" | "$bin" pre-receive --root "$root" --actor "${JERYU_ACTOR:-wire-client}" "$owner" "$repo"
fi
"#;

/// Pre-receive guard for protected refs and quarantine object validation.
#[derive(Clone, Debug)]
pub struct PreReceiveGuard {
    rules: Vec<ProtectedRefRule>,
    fsck: ObjectFsck,
}

impl PreReceiveGuard {
    /// Create a guard.
    #[must_use]
    pub fn new(rules: Vec<ProtectedRefRule>, fsck: ObjectFsck) -> Self {
        Self { rules, fsck }
    }

    /// Evaluate newline-separated `prior next ref` triplets.
    pub fn evaluate_lines(
        &self,
        repo: &Repository,
        actor: &str,
        input: &str,
    ) -> Result<Vec<RefChange>> {
        let mut changes = Vec::new();
        for (idx, line) in input.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 3 {
                return Err(GitdError::InvalidInput(format!(
                    "pre-receive line {} must be: <prior> <next> <ref>",
                    idx + 1
                )));
            }
            let old_oid = parts[0];
            let new_oid = parts[1];
            let ref_name = parts[2];
            validate_object_id(old_oid, idx + 1, "prior")?;
            validate_object_id(new_oid, idx + 1, "next")?;
            validate_ref_name(ref_name)?;
            let operation = if is_zero_oid(new_oid) {
                RefOperation::Delete
            } else if is_zero_oid(old_oid) {
                RefOperation::Create
            } else {
                RefOperation::Update
            };
            if ref_name == MAIN_REF {
                return Err(GitdError::ProtectedRefDenied(
                    "direct pushes to refs/heads/main are blocked; open a pull request and merge through Jeryu".to_string(),
                ));
            }
            let force = operation == RefOperation::Update
                && !self
                    .fsck
                    .is_ancestor(repo, old_oid, new_oid)
                    .unwrap_or(false);
            let change = RefChange {
                actor: actor.to_string(),
                ref_name: ref_name.to_string(),
                old_oid: old_oid.to_string(),
                new_oid: new_oid.to_string(),
                operation,
                force,
            };
            for rule in &self.rules {
                rule.evaluate(&change)?;
            }
            changes.push(change);
        }
        let new_oids: Vec<String> = changes
            .iter()
            .filter(|change| !is_zero_oid(&change.new_oid))
            .map(|change| change.new_oid.clone())
            .collect();
        self.fsck
            .reject_oversized_raw_blobs(repo, &new_oids, RAW_GIT_BLOB_LIMIT_BYTES)?;
        self.fsck.fsck(repo)?;
        Ok(changes)
    }

    /// Read stdin and evaluate it for a Git pre-receive hook.
    pub fn evaluate_stdin(&self, repo: &Repository, actor: &str) -> Result<Vec<RefChange>> {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        self.evaluate_lines(repo, actor, &input)
    }
}

fn validate_object_id(oid: &str, line: usize, field: &str) -> Result<()> {
    if oid.len() == 40 && oid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }
    Err(GitdError::InvalidInput(format!(
        "pre-receive line {line} has invalid {field} oid"
    )))
}
