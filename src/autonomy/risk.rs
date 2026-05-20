//! Path-based + heuristic risk classifier (R0..R5).
//!
//! Walks the `.jeryu/autonomy/policies/risk.yml` tier list in declared order and
//! returns the first matching tier. The YAML lists tiers top-down most-
//! restrictive-first (R5, R4, R3, R2, R1, R0), so the first match wins by
//! design — that's how veto semantics survive: a hard-stop tier can't be
//! undone by a "lower" tier matching later.

use crate::autonomy::policy_yaml::{PolicyBundle, RiskMatcher};
use crate::autonomy::types::{ChangedFile, RiskTier};
use regex::Regex;

pub struct RiskClassifier<'a> {
    pub policy: &'a PolicyBundle,
}

pub struct ClassificationInputs<'a> {
    pub files: &'a [ChangedFile],
    /// Triggered names from the conditions registry (e.g. `lockfile_only_change`).
    pub triggered_conditions: &'a [String],
}

impl<'a> RiskClassifier<'a> {
    pub fn new(policy: &'a PolicyBundle) -> Self {
        Self { policy }
    }

    pub fn classify(&self, inp: &ClassificationInputs<'_>) -> RiskTier {
        let protected_globs: Vec<Regex> = self
            .policy
            .protected_paths
            .hard_human
            .iter()
            .filter_map(|g| compile_glob(g).ok())
            .collect();
        for tier in &self.policy.risk.tiers {
            for m in &tier.matchers {
                if matcher_matches(m, inp, &protected_globs) {
                    return tier.id;
                }
            }
        }
        RiskTier::R2
    }
}

fn matcher_matches(
    m: &RiskMatcher,
    inp: &ClassificationInputs<'_>,
    protected_globs: &[Regex],
) -> bool {
    if m.default {
        return true;
    }
    if let Some(true) = m.any_path_matches_protected
        && !inp
            .files
            .iter()
            .any(|f| protected_globs.iter().any(|r| r.is_match(&f.path)))
    {
        return false;
    }
    if !m.paths_match.is_empty() {
        let globs: Vec<Regex> = m
            .paths_match
            .iter()
            .filter_map(|g| compile_glob(g).ok())
            .collect();
        if !inp
            .files
            .iter()
            .any(|f| globs.iter().any(|r| r.is_match(&f.path)))
        {
            return false;
        }
    }
    if !m.paths_only_in.is_empty() {
        let globs: Vec<Regex> = m
            .paths_only_in
            .iter()
            .filter_map(|g| compile_glob(g).ok())
            .collect();
        if !inp
            .files
            .iter()
            .all(|f| globs.iter().any(|r| r.is_match(&f.path)))
        {
            return false;
        }
    }
    let total_lines: u32 = inp
        .files
        .iter()
        .map(|f| f.lines_added + f.lines_removed)
        .sum();
    if let Some(max) = m.max_lines_changed
        && total_lines > max
    {
        return false;
    }
    if let Some(gte) = m.lines_changed_gte
        && total_lines < gte
    {
        return false;
    }
    if let Some(lte) = m.lines_changed_lte
        && total_lines > lte
    {
        return false;
    }
    // `all_files_have_targeted_tests` requires test-mapping context we don't
    // have here. Conservative: when present, this matcher never fires from the
    // pure classifier; the orchestrator can pre-validate and set a synthetic
    // condition in `triggered_conditions`.
    if m.all_files_have_targeted_tests == Some(true)
        && !inp
            .triggered_conditions
            .iter()
            .any(|c| c == "all_files_have_targeted_tests")
    {
        return false;
    }
    if !m.conditions.is_empty()
        && !m
            .conditions
            .iter()
            .all(|c| inp.triggered_conditions.iter().any(|t| t == c))
    {
        return false;
    }
    // If we got here AND the matcher has at least one constraint that
    // succeeded, accept. Empty matchers (no constraints, no default) are
    // never true — they're treated as malformed config.

    !m.paths_match.is_empty()
        || !m.paths_only_in.is_empty()
        || !m.conditions.is_empty()
        || m.max_lines_changed.is_some()
        || m.lines_changed_gte.is_some()
        || m.lines_changed_lte.is_some()
        || m.any_path_matches_protected == Some(true)
        || m.all_files_have_targeted_tests == Some(true)
}

/// Convert a gitignore-style glob to a Rust regex. Supports `**`, `*`, `?`.
pub fn compile_glob(glob: &str) -> Result<Regex, regex::Error> {
    let mut r = String::with_capacity(glob.len() + 4);
    r.push('^');
    let bytes = glob.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            r.push_str(".*");
            i += 2;
            if i < bytes.len() && bytes[i] == b'/' {
                i += 1;
            }
            continue;
        }
        match c {
            b'*' => {
                r.push_str("[^/]*");
                i += 1;
            }
            b'?' => {
                r.push_str("[^/]");
                i += 1;
            }
            b'.' | b'+' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'^' | b'$' | b'|' | b'\\' => {
                r.push('\\');
                r.push(c as char);
                i += 1;
            }
            _ => {
                r.push(c as char);
                i += 1;
            }
        }
    }
    r.push('$');
    Regex::new(&r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cf(path: &str, added: u32, removed: u32) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            risk_tags: vec![],
            lines_added: added,
            lines_removed: removed,
        }
    }

    fn bundle() -> PolicyBundle {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".jeryu/autonomy/policies");
        PolicyBundle::from_dir(&dir).expect("loads")
    }

    #[test]
    fn protected_path_lands_in_r4() {
        let b = bundle();
        let cls = RiskClassifier::new(&b);
        let files = [cf("CODEOWNERS", 1, 0)];
        let t = cls.classify(&ClassificationInputs {
            files: &files,
            triggered_conditions: &[],
        });
        assert_eq!(t, RiskTier::R4, "CODEOWNERS must escalate to R4");
    }

    #[test]
    fn autonomy_path_lands_in_r4() {
        let b = bundle();
        let cls = RiskClassifier::new(&b);
        let files = [cf(".jeryu/autonomy/policies/approvals.yml", 3, 1)];
        let t = cls.classify(&ClassificationInputs {
            files: &files,
            triggered_conditions: &[],
        });
        assert_eq!(t, RiskTier::R4);
    }

    #[test]
    fn docs_only_change_lands_in_r0() {
        let b = bundle();
        let cls = RiskClassifier::new(&b);
        let files = [cf("docs/some.md", 5, 0), cf("README.md", 1, 0)];
        let t = cls.classify(&ClassificationInputs {
            files: &files,
            triggered_conditions: &[],
        });
        assert_eq!(t, RiskTier::R0, "docs-only should be R0, got {:?}", t);
    }

    #[test]
    fn r5_condition_supersedes_protected_path() {
        let b = bundle();
        let cls = RiskClassifier::new(&b);
        let files = [cf("CODEOWNERS", 1, 0)];
        let triggered = ["evidence_missing".to_string()];
        let t = cls.classify(&ClassificationInputs {
            files: &files,
            triggered_conditions: &triggered,
        });
        assert_eq!(t, RiskTier::R5);
    }

    #[test]
    fn small_change_with_targeted_tests_lands_in_r1() {
        let b = bundle();
        let cls = RiskClassifier::new(&b);
        let files = [cf("src/util.rs", 30, 5)];
        let triggered = ["all_files_have_targeted_tests".to_string()];
        let t = cls.classify(&ClassificationInputs {
            files: &files,
            triggered_conditions: &triggered,
        });
        assert_eq!(t, RiskTier::R1, "small + tests should be R1");
    }

    #[test]
    fn glob_star_matches_one_segment() {
        let r = compile_glob("src/*.rs").unwrap();
        assert!(r.is_match("src/foo.rs"));
        assert!(!r.is_match("src/sub/bar.rs"));
    }

    #[test]
    fn glob_double_star_matches_many_segments() {
        let r = compile_glob("src/**/*.rs").unwrap();
        assert!(r.is_match("src/foo.rs"));
        assert!(r.is_match("src/sub/bar.rs"));
        assert!(r.is_match("src/a/b/c/d.rs"));
        assert!(!r.is_match("crates/x.rs"));
    }

    #[test]
    fn empty_matcher_is_inert() {
        let m = RiskMatcher::default();
        let inp = ClassificationInputs {
            files: &[],
            triggered_conditions: &[],
        };
        let protected_globs: Vec<Regex> = vec![];
        assert!(!matcher_matches(&m, &inp, &protected_globs));
    }
}
