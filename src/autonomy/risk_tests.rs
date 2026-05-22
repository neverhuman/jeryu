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
