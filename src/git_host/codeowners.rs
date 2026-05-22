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

#[cfg(test)]
#[path = "codeowners_tests.rs"]
mod tests;

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
