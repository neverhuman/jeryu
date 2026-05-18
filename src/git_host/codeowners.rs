//! Owner: Evidence Gate / git host adapter plane
//! Proof: `cargo test -p jeryu -- git_host::codeowners`
//! Invariants:
//!   - Last-matching rule wins (per GitHub/GitLab CODEOWNERS spec).
//!   - Owners can be `@user`, `@org/team`, or an email address.
//!   - Patterns are leading-`/` anchored to repo root, otherwise match anywhere.
//!   - `**` matches across path separators; `*` matches within one segment.
//!
//! Minimal parser sufficient for the cross-check the Judge needs: which
//! CODEOWNER teams must approve a given changed-path set, and is at least
//! one approver from each required team present?

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct CodeOwners {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
struct Rule {
    pattern: String,
    owners: Vec<String>,
}

impl CodeOwners {
    /// Parse a CODEOWNERS file. Comments (lines starting with `#`) and blank
    /// lines are ignored. Each non-empty line is `pattern owner1 owner2 ...`.
    pub fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let owners: Vec<String> = parts
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            if owners.is_empty() {
                // Pattern with no owners explicitly removes ownership for that path.
                rules.push(Rule {
                    pattern: pattern.to_string(),
                    owners: vec![],
                });
            } else {
                rules.push(Rule {
                    pattern: pattern.to_string(),
                    owners,
                });
            }
        }
        Self { rules }
    }

    /// Return the owners for one path, applying last-match-wins.
    /// `None` means no rule matched; `Some(vec![])` means a rule explicitly
    /// cleared ownership.
    pub fn owners_for(&self, path: &str) -> Option<&[String]> {
        let mut hit: Option<&Rule> = None;
        for rule in &self.rules {
            if pattern_matches(&rule.pattern, path) {
                hit = Some(rule);
            }
        }
        hit.map(|r| r.owners.as_slice())
    }

    /// Cross-check changed paths against present approver identities.
    /// `approvers` is the set of agent_id / login strings that approved
    /// (e.g. `["@alice", "@org/security"]` — owners and approvers must
    /// share string format).
    pub fn check(&self, changed_paths: &[&str], approvers: &HashSet<String>) -> CodeOwnersCheck {
        let mut required: HashMap<String, Vec<String>> = HashMap::new();
        let mut unsatisfied: Vec<String> = Vec::new();
        for path in changed_paths {
            let Some(owners) = self.owners_for(path) else {
                continue;
            };
            if owners.is_empty() {
                continue; // explicitly cleared
            }
            required.insert(path.to_string(), owners.to_vec());
            let satisfied = owners.iter().any(|o| approvers.contains(o));
            if !satisfied {
                unsatisfied.push(path.to_string());
            }
        }
        if unsatisfied.is_empty() {
            CodeOwnersCheck::Satisfied
        } else {
            CodeOwnersCheck::Unsatisfied {
                unsatisfied_paths: unsatisfied,
                required_owners: required,
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeOwnersCheck {
    Satisfied,
    Unsatisfied {
        unsatisfied_paths: Vec<String>,
        required_owners: HashMap<String, Vec<String>>,
    },
}

impl CodeOwnersCheck {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, CodeOwnersCheck::Satisfied)
    }
}

/// Minimal CODEOWNERS pattern matcher.
/// Supports: leading `/` (root-anchored), trailing `/` (directory),
/// `*` (within-segment), `**` (across segments). Patterns without leading `/`
/// match against any suffix of the path.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    // Directory rule: `foo/` matches `foo/anything`.
    let (pattern, dir_only) = if let Some(stripped) = pattern.strip_suffix('/') {
        (stripped.to_string(), true)
    } else {
        (pattern.to_string(), false)
    };
    if dir_only {
        let needle = format!("{pattern}/");
        // Anchored at root if pattern starts with `/`.
        if let Some(rest) = pattern.strip_prefix('/') {
            return path.starts_with(&format!("{rest}/")) || path == rest;
        }
        return path.contains(&needle) || path.starts_with(&format!("{pattern}/"));
    }
    let (pattern, anchored) = if let Some(stripped) = pattern.strip_prefix('/') {
        (stripped.to_string(), true)
    } else {
        (pattern, false)
    };
    if anchored {
        glob_match(&pattern, path)
    } else {
        // Match against any suffix that aligns to a path boundary.
        if glob_match(&pattern, path) {
            return true;
        }
        for (i, c) in path.char_indices() {
            if c == '/' && glob_match(&pattern, &path[i + 1..]) {
                return true;
            }
        }
        false
    }
}

fn glob_match(pattern: &str, path: &str) -> bool {
    glob_inner(pattern.as_bytes(), 0, path.as_bytes(), 0)
}

fn glob_inner(p: &[u8], pi: usize, s: &[u8], si: usize) -> bool {
    let mut pi = pi;
    let mut si = si;
    while pi < p.len() {
        if p[pi] == b'*' {
            // Detect `**`.
            let double = pi + 1 < p.len() && p[pi + 1] == b'*';
            if double {
                pi += 2;
                // Optional separator after `**`.
                if pi < p.len() && p[pi] == b'/' {
                    pi += 1;
                }
                if pi >= p.len() {
                    return true; // `**` at end matches all
                }
                // Try every position from si onward.
                for try_si in si..=s.len() {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            } else {
                pi += 1;
                if pi >= p.len() {
                    // `*` at end matches the remainder up to the next `/`.
                    return !s[si..].contains(&b'/');
                }
                // Try every position from si until next separator.
                let limit = s[si..]
                    .iter()
                    .position(|c| *c == b'/')
                    .map(|n| si + n)
                    .unwrap_or(s.len());
                for try_si in si..=limit {
                    if glob_inner(p, pi, s, try_si) {
                        return true;
                    }
                }
                return false;
            }
        } else if si < s.len() && p[pi] == s[si] {
            pi += 1;
            si += 1;
        } else {
            return false;
        }
    }
    si == s.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_rules() {
        let co = CodeOwners::parse(
            "
            # comment
            * @core-team
            /docs/ @docs-team
            *.rs @rust-team @alice
            ",
        );
        assert_eq!(co.rules.len(), 3);
    }

    #[test]
    fn last_match_wins() {
        let co = CodeOwners::parse(
            "
            * @core-team
            /src/auth/** @security
            ",
        );
        assert_eq!(
            co.owners_for("src/auth/login.rs"),
            Some(&["@security".to_string()][..])
        );
        assert_eq!(
            co.owners_for("src/foo.rs"),
            Some(&["@core-team".to_string()][..])
        );
    }

    #[test]
    fn directory_rule_matches_contents() {
        let co = CodeOwners::parse("/docs/ @docs-team");
        assert_eq!(
            co.owners_for("docs/intro.md"),
            Some(&["@docs-team".to_string()][..])
        );
        assert_eq!(co.owners_for("src/foo.rs"), None);
    }

    #[test]
    fn glob_double_star_matches_across_segments() {
        let co = CodeOwners::parse("**/migrations/** @db-team");
        assert_eq!(
            co.owners_for("services/cart/migrations/20240101_init.sql"),
            Some(&["@db-team".to_string()][..])
        );
    }

    #[test]
    fn check_satisfied_when_owner_approves() {
        let co = CodeOwners::parse("/src/auth/** @security");
        let approvers: HashSet<String> = ["@security".into()].into_iter().collect();
        let result = co.check(&["src/auth/login.rs"], &approvers);
        assert_eq!(result, CodeOwnersCheck::Satisfied);
    }

    #[test]
    fn check_unsatisfied_when_no_owner_approves() {
        let co = CodeOwners::parse("/src/auth/** @security");
        let approvers: HashSet<String> = ["@alice".into()].into_iter().collect();
        let result = co.check(&["src/auth/login.rs"], &approvers);
        match result {
            CodeOwnersCheck::Unsatisfied {
                unsatisfied_paths,
                required_owners,
            } => {
                assert_eq!(unsatisfied_paths, vec!["src/auth/login.rs".to_string()]);
                assert_eq!(
                    required_owners.get("src/auth/login.rs"),
                    Some(&vec!["@security".to_string()])
                );
            }
            _ => panic!("expected Unsatisfied"),
        }
    }

    #[test]
    fn paths_without_owners_are_ignored() {
        let co = CodeOwners::parse("/src/auth/** @security");
        let approvers: HashSet<String> = HashSet::new();
        let result = co.check(&["README.md"], &approvers);
        assert_eq!(result, CodeOwnersCheck::Satisfied);
    }

    #[test]
    fn explicitly_cleared_owners_skip_check() {
        let co = CodeOwners::parse(
            "
            /src/auth/** @security
            /src/auth/public/**
            ",
        );
        // The cleared rule wins last; public/ has no required owners.
        let approvers: HashSet<String> = HashSet::new();
        let result = co.check(&["src/auth/public/widget.rs"], &approvers);
        assert_eq!(result, CodeOwnersCheck::Satisfied);
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// Deeply nested paths (six+ segments) must be matched by `**`
    /// patterns regardless of how many `/` separators the path contains.
    /// This is the canonical recursive-glob check: the matcher must not
    /// silently bound by segment count.
    #[test]
    fn deep_nesting_matched_by_double_star_pattern() {
        let co = CodeOwners::parse("/services/**/migrations/** @db-team");
        let nested_path = "services/cart/web/api/v2/migrations/2026/01/01/init.sql";
        assert_eq!(
            co.owners_for(nested_path),
            Some(&["@db-team".to_string()][..]),
            "deeply nested path must still match `**/migrations/**`"
        );
        // Even deeper.
        let deeper = "services/a/b/c/d/e/f/g/migrations/h/i/j/k.sql";
        assert_eq!(co.owners_for(deeper), Some(&["@db-team".to_string()][..]),);
    }

    /// Multiple owners on the same line (whitespace-separated) must all be
    /// recorded and any one of them satisfies the check.
    #[test]
    fn multiple_owners_on_same_line_all_recorded_any_satisfies() {
        let co = CodeOwners::parse(
            "
            /src/auth/** @security @platform @core-team alice@example.com
            ",
        );
        let owners = co.owners_for("src/auth/login.rs").expect("rule must match");
        assert_eq!(
            owners,
            &[
                "@security".to_string(),
                "@platform".to_string(),
                "@core-team".to_string(),
                "alice@example.com".to_string(),
            ]
        );
        // Any single approver from the set satisfies.
        for approver in ["@security", "@platform", "@core-team", "alice@example.com"] {
            let approvers: HashSet<String> = [approver.to_string()].into_iter().collect();
            let result = co.check(&["src/auth/login.rs"], &approvers);
            assert_eq!(
                result,
                CodeOwnersCheck::Satisfied,
                "approver `{approver}` should satisfy the rule"
            );
        }
        // An unrelated approver does NOT satisfy.
        let approvers: HashSet<String> = ["@stranger".into()].into_iter().collect();
        let result = co.check(&["src/auth/login.rs"], &approvers);
        assert!(!result.is_satisfied());
    }

    #[test]
    fn check_is_satisfied_truthy_helper() {
        assert!(CodeOwnersCheck::Satisfied.is_satisfied());
        assert!(
            !CodeOwnersCheck::Unsatisfied {
                unsatisfied_paths: vec!["x".into()],
                required_owners: HashMap::new(),
            }
            .is_satisfied()
        );
    }
}
