//! Git ref operations.

use crate::command::run_capture;
use crate::error::{GitdError, Result};
use crate::protection::{RefChange, RefOperation};
use crate::repo::{RepoManager, Repository};

/// A Git ref and object id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitRef {
    /// Ref name such as `refs/heads/main`.
    pub name: String,
    /// Object id as hex.
    pub oid: String,
}

/// Ref service enforcing protected-ref policy before mutation.
#[derive(Clone, Debug)]
pub struct RefService {
    manager: RepoManager,
}

impl RefService {
    /// Create a ref service.
    #[must_use]
    pub fn new(manager: RepoManager) -> Self {
        Self { manager }
    }

    /// List refs in a bare repository.
    pub fn list_refs(&self, repo: &Repository) -> Result<Vec<GitRef>> {
        let out = run_capture(
            &self.manager.config().git_bin,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
            Some(&repo.path),
        )?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut refs = Vec::new();
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let Some(oid) = parts.next() else { continue };
            refs.push(GitRef {
                name: name.to_string(),
                oid: oid.to_string(),
            });
        }
        Ok(refs)
    }

    /// Update a ref with policy checks.
    pub fn update_ref(
        &self,
        repo: &Repository,
        actor: &str,
        name: &str,
        new_oid: &str,
        old_oid: Option<&str>,
    ) -> Result<()> {
        validate_ref_name(name)?;
        let operation = if is_zero_oid(new_oid) {
            RefOperation::Delete
        } else if old_oid.is_some() {
            RefOperation::Update
        } else {
            RefOperation::Create
        };
        let change = RefChange {
            actor: actor.to_string(),
            ref_name: name.to_string(),
            old_oid: old_oid.unwrap_or(ZERO_OID).to_string(),
            new_oid: new_oid.to_string(),
            operation,
            force: false,
        };
        for rule in &self.manager.config().protected_refs {
            rule.evaluate(&change)?;
        }
        let mut args = vec!["update-ref", name, new_oid];
        if let Some(old) = old_oid {
            args.push(old);
        }
        run_capture(&self.manager.config().git_bin, &args, Some(&repo.path))?;
        Ok(())
    }
}

/// All-zero oid used by receive-pack for deletes.
pub const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// Validate a ref name using Git itself plus local sanity guards.
pub fn validate_ref_name(name: &str) -> Result<()> {
    if name.contains('\0') || name.starts_with('-') {
        return Err(GitdError::InvalidInput("invalid ref name".to_string()));
    }
    run_capture("git", &["check-ref-format", "--allow-onelevel", name], None).map(|_| ())
}

/// Whether an oid is all zeroes.
#[must_use]
pub fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.chars().all(|c| c == '0')
}
