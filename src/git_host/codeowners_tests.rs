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
