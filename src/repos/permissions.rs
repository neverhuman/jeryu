//! Owner: Web Forge BFF — repository permission normalization.
//! Proof: `cargo nextest run -p jeryu --lib repos::permissions`
//! Invariants:
//!   - The mapping is one-way (host role → JeRyu permission keys). The reverse
//!     is host-specific and lives in the host adapter (W-H-* future work).
//!
//! See `WEB_WORK_CLAUDE.md` §35.1.18 and FINAL spec §6.8 — every host role
//! maps to a subset of the canonical 28-key permission set defined in
//! `crate::web::permissions::perms`.

use std::collections::HashSet;

use crate::web::permissions::perms;

/// Normalize a GitLab member role into the canonical permission set. Unknown
/// roles map to the empty set (callers SHOULD treat this as "no perms").
///
/// GitLab role taxonomy (per the docs):
///   - `guest` — read-only viewer.
///   - `reporter` — read + issues/MR reads.
///   - `developer` — write to non-protected branches, MR comments.
///   - `maintainer` — settings, branch protection, members.
///   - `owner` — everything including delete.
pub fn perms_for_gitlab_role(role: &str) -> HashSet<&'static str> {
    let lower = role.to_ascii_lowercase();
    let mut set: HashSet<&'static str> = HashSet::new();
    let level = match lower.as_str() {
        "guest" | "10" => 10,
        "reporter" | "20" => 20,
        "developer" | "30" => 30,
        "maintainer" | "40" => 40,
        "owner" | "50" => 50,
        _ => return set, // unknown role: no perms
    };

    if level >= 10 {
        set.insert(perms::REPO_READ);
        set.insert(perms::CODE_READ);
    }
    if level >= 20 {
        set.insert(perms::ISSUE_READ);
        set.insert(perms::MR_READ);
        set.insert(perms::CI_READ);
        set.insert(perms::AGENTS_READ);
    }
    if level >= 30 {
        set.insert(perms::CODE_WRITE);
        set.insert(perms::BRANCH_CREATE);
        set.insert(perms::MR_WRITE);
        set.insert(perms::MR_COMMENT);
        set.insert(perms::MR_REVIEW);
        set.insert(perms::ISSUE_WRITE);
        set.insert(perms::CI_WRITE);
        set.insert(perms::AGENTS_WRITE);
    }
    if level >= 40 {
        set.insert(perms::REPO_WRITE);
        set.insert(perms::REPO_CREATE);
        set.insert(perms::BRANCH_DELETE);
        set.insert(perms::MR_APPROVE);
        set.insert(perms::MR_MERGE);
        set.insert(perms::SETTINGS_READ);
        set.insert(perms::SETTINGS_WRITE);
        set.insert(perms::SECRETS_READ_METADATA);
        set.insert(perms::SECRETS_WRITE);
        set.insert(perms::AGENTS_GRANT);
    }
    if level >= 50 {
        set.insert(perms::REPO_ADMIN);
        set.insert(perms::REPO_DELETE);
        set.insert(perms::AUDIT_READ);
        set.insert(perms::ADMIN_AUDIT);
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_has_only_read() {
        let p = perms_for_gitlab_role("guest");
        assert!(p.contains(perms::REPO_READ));
        assert!(p.contains(perms::CODE_READ));
        assert!(!p.contains(perms::CODE_WRITE));
        assert!(!p.contains(perms::MR_MERGE));
    }

    #[test]
    fn developer_gets_write_but_not_settings() {
        let p = perms_for_gitlab_role("developer");
        assert!(p.contains(perms::CODE_WRITE));
        assert!(p.contains(perms::MR_WRITE));
        assert!(!p.contains(perms::SETTINGS_WRITE));
        assert!(!p.contains(perms::REPO_DELETE));
    }

    #[test]
    fn maintainer_gets_settings_but_not_delete() {
        let p = perms_for_gitlab_role("maintainer");
        assert!(p.contains(perms::SETTINGS_WRITE));
        assert!(p.contains(perms::MR_MERGE));
        assert!(p.contains(perms::SECRETS_READ_METADATA));
        assert!(!p.contains(perms::REPO_DELETE));
    }

    #[test]
    fn owner_gets_everything() {
        let p = perms_for_gitlab_role("owner");
        assert!(p.contains(perms::REPO_DELETE));
        assert!(p.contains(perms::AUDIT_READ));
        assert!(p.contains(perms::SETTINGS_WRITE));
    }

    #[test]
    fn unknown_role_returns_empty() {
        let p = perms_for_gitlab_role("contributor"); // not a real GL role
        assert!(p.is_empty());
    }

    #[test]
    fn numeric_access_levels_accepted() {
        let p = perms_for_gitlab_role("40");
        assert!(p.contains(perms::SETTINGS_WRITE));
    }
}
