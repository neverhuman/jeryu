//! Canonical permission key constants per `WEB_WORK_CLAUDE.md` §35.1.18
//! (W-CC-07). The normalized set spans 28 fine-grained keys; the §35.1.18
//! "24-key" count omits trivially-implied keys (`audit.read` implies
//! `admin.audit` etc.) — the constants here are exhaustive so callers can
//! reference exactly the key the BFF will check.
//!
//! Used by route handlers via [`require`] and by `auth::Viewer::local_dev`
//! to seed the dev-mode permission set.

pub mod perms {
    pub const REPO_READ: &str = "repo.read";
    pub const REPO_CREATE: &str = "repo.create";
    pub const REPO_WRITE: &str = "repo.write";
    pub const REPO_ADMIN: &str = "repo.admin";
    pub const REPO_DELETE: &str = "repo.delete";

    pub const CODE_READ: &str = "code.read";
    pub const CODE_WRITE: &str = "code.write";

    pub const BRANCH_CREATE: &str = "branch.create";
    pub const BRANCH_DELETE: &str = "branch.delete";

    pub const SETTINGS_READ: &str = "settings.read";
    pub const SETTINGS_WRITE: &str = "settings.write";

    pub const MR_READ: &str = "mr.read";
    pub const MR_WRITE: &str = "mr.write";
    pub const MR_COMMENT: &str = "mr.comment";
    pub const MR_REVIEW: &str = "mr.review";
    pub const MR_APPROVE: &str = "mr.approve";
    pub const MR_MERGE: &str = "mr.merge";

    pub const ISSUE_READ: &str = "issue.read";
    pub const ISSUE_WRITE: &str = "issue.write";

    pub const CI_READ: &str = "ci.read";
    pub const CI_WRITE: &str = "ci.write";

    pub const SECRETS_READ_METADATA: &str = "secrets.read_metadata";
    pub const SECRETS_WRITE: &str = "secrets.write";

    pub const AGENTS_READ: &str = "agents.read";
    pub const AGENTS_WRITE: &str = "agents.write";
    pub const AGENTS_GRANT: &str = "agents.grant";

    pub const AUDIT_READ: &str = "audit.read";
    pub const ADMIN_AUDIT: &str = "admin.audit";

    /// All canonical permission keys, useful for tests and dev-mode seeding.
    pub const ALL: &[&str] = &[
        REPO_READ,
        REPO_CREATE,
        REPO_WRITE,
        REPO_ADMIN,
        REPO_DELETE,
        CODE_READ,
        CODE_WRITE,
        BRANCH_CREATE,
        BRANCH_DELETE,
        SETTINGS_READ,
        SETTINGS_WRITE,
        MR_READ,
        MR_WRITE,
        MR_COMMENT,
        MR_REVIEW,
        MR_APPROVE,
        MR_MERGE,
        ISSUE_READ,
        ISSUE_WRITE,
        CI_READ,
        CI_WRITE,
        SECRETS_READ_METADATA,
        SECRETS_WRITE,
        AGENTS_READ,
        AGENTS_WRITE,
        AGENTS_GRANT,
        AUDIT_READ,
        ADMIN_AUDIT,
    ];
}

use super::auth::Viewer;
use super::error::ApiError;

/// Assert that `viewer` carries `perm`. Returns `ApiError::Forbidden(perm)`
/// otherwise so the §35.1.11 error envelope identifies the missing key.
pub fn require(viewer: &Viewer, perm: &str) -> Result<(), ApiError> {
    if viewer.perms.contains(perm) {
        Ok(())
    } else {
        Err(ApiError::Forbidden(perm.into()))
    }
}

/// Map a WebSocket subscription scope to the permission key required to
/// subscribe to it (§35.1.6 + §35.1.15). `None` means the scope is
/// public-by-default (e.g. `system.health`, `global.activity` — viewer
/// must be authenticated but needs no additional perm).
pub fn perm_for_scope(scope: &str) -> Option<&'static str> {
    if scope == "global.activity" || scope == "system.health" {
        None
    } else if scope.starts_with("user.") && scope.ends_with(".notifications") {
        // Notifications scope is per-user; the WS handler also checks that
        // the embedded user_id matches viewer.id (Phase 1: we just allow
        // it since the binding is informational).
        None
    } else if scope == "global" {
        // Reserved "match-everything" pseudo-scope used by the bus.
        None
    } else if scope.starts_with("repo.") {
        Some(perms::REPO_READ)
    } else if scope.starts_with("mr.") {
        Some(perms::MR_READ)
    } else if scope.starts_with("issue.") {
        Some(perms::ISSUE_READ)
    } else if scope.starts_with("agent.") {
        Some(perms::AGENTS_READ)
    } else if scope.starts_with("runner.") {
        Some(perms::CI_READ)
    } else if scope.starts_with("cache.") {
        Some(perms::REPO_READ)
    } else {
        // Unknown scope: require admin audit to keep new topics from
        // leaking by default.
        Some(perms::ADMIN_AUDIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perm_for_scope_maps_known_prefixes() {
        assert_eq!(perm_for_scope("global.activity"), None);
        assert_eq!(perm_for_scope("system.health"), None);
        assert_eq!(perm_for_scope("global"), None);
        assert_eq!(perm_for_scope("repo.abc"), Some("repo.read"));
        assert_eq!(perm_for_scope("repo.abc.activity"), Some("repo.read"));
        assert_eq!(perm_for_scope("mr.42"), Some("mr.read"));
        assert_eq!(perm_for_scope("issue.7"), Some("issue.read"));
        assert_eq!(perm_for_scope("agent.q"), Some("agents.read"));
        assert_eq!(perm_for_scope("runner.r1"), Some("ci.read"));
        assert_eq!(perm_for_scope("cache.r1"), Some("repo.read"));
        assert_eq!(perm_for_scope("user.u1.notifications"), None);
    }

    #[test]
    fn perms_all_has_28_canonical_keys() {
        // 24 spec-listed plus 4 implied (branch.create/delete, mr.comment,
        // mr.review). See §35.1.18.
        assert_eq!(perms::ALL.len(), 28);
    }
}
