//! Protected-ref policy.

use crate::error::{GitdError, Result};

/// Operation type for a ref update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefOperation {
    /// A new ref is being created.
    Create,
    /// An existing ref is being updated.
    Update,
    /// A ref is being deleted.
    Delete,
}

/// Proposed ref change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefChange {
    /// Actor identity.
    pub actor: String,
    /// Full ref name.
    pub ref_name: String,
    /// Previous object id.
    pub old_oid: String,
    /// New object id.
    pub new_oid: String,
    /// Operation.
    pub operation: RefOperation,
    /// Whether the update is known to be forceful.
    pub force: bool,
}

/// Rule for protected refs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedRefRule {
    /// Glob-like pattern. Supports suffix `*`.
    pub pattern: String,
    /// Deny deletes.
    pub deny_delete: bool,
    /// Deny force updates.
    pub deny_force: bool,
    /// Actors exempt from this rule.
    pub bypass_actors: Vec<String>,
}

impl ProtectedRefRule {
    /// Default Phase 1 rules.
    ///
    /// PR auto-merge ([`crate::refs::RefService::merge_pull`]) advances the base
    /// ref as a non-force fast-forward-of-base (the new tip is always a
    /// descendant of the old base), so it satisfies `deny_force` on
    /// `refs/heads/main` without needing a bypass actor here.
    #[must_use]
    pub fn default_phase1_rules() -> Vec<Self> {
        vec![
            Self {
                pattern: "refs/heads/main".to_string(),
                deny_delete: true,
                deny_force: true,
                bypass_actors: vec!["system:mirror".to_string()],
            },
            Self {
                pattern: "refs/tags/*".to_string(),
                deny_delete: true,
                deny_force: true,
                bypass_actors: Vec::new(),
            },
        ]
    }

    /// Evaluate a change against the rule.
    pub fn evaluate(&self, change: &RefChange) -> Result<()> {
        if !matches_pattern(&self.pattern, &change.ref_name)
            || self.bypass_actors.iter().any(|a| a == &change.actor)
        {
            return Ok(());
        }
        if self.deny_delete && change.operation == RefOperation::Delete {
            return Err(GitdError::ProtectedRefDenied(format!(
                "{} cannot delete {}",
                change.actor, change.ref_name
            )));
        }
        if self.deny_force && change.force {
            return Err(GitdError::ProtectedRefDenied(format!(
                "{} cannot force-update {}",
                change.actor, change.ref_name
            )));
        }
        Ok(())
    }
}

/// Very small glob matcher used for protected ref patterns.
#[must_use]
pub fn matches_pattern(pattern: &str, value: &str) -> bool {
    if pattern == value {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    false
}
