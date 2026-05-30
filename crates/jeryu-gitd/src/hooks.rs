//! Server-side hook implementations.

use crate::error::{GitdError, Result};
use crate::object_fsck::ObjectFsck;
use crate::protection::{ProtectedRefRule, RefChange, RefOperation};
use crate::refs::{is_zero_oid, validate_ref_name};
use crate::repo::Repository;
use std::io::{self, Read};

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

    /// Evaluate newline-separated `old new ref` triplets.
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
                    "pre-receive line {} must be: <old> <new> <ref>",
                    idx + 1
                )));
            }
            let old_oid = parts[0];
            let new_oid = parts[1];
            let ref_name = parts[2];
            validate_object_id(old_oid, idx + 1, "old")?;
            validate_object_id(new_oid, idx + 1, "new")?;
            validate_ref_name(ref_name)?;
            let operation = if is_zero_oid(new_oid) {
                RefOperation::Delete
            } else if is_zero_oid(old_oid) {
                RefOperation::Create
            } else {
                RefOperation::Update
            };
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
