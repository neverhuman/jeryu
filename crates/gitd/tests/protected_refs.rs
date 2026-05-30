use gitd::protection::{ProtectedRefRule, RefChange, RefOperation};

#[test]
fn protected_ref_denies_delete() {
    let rule = ProtectedRefRule::default_phase1_rules().remove(0);
    let change = RefChange {
        actor: "alice".to_string(),
        ref_name: "refs/heads/main".to_string(),
        old_oid: "abc".to_string(),
        new_oid: "0000000000000000000000000000000000000000".to_string(),
        operation: RefOperation::Delete,
        force: false,
    };
    assert!(rule.evaluate(&change).is_err());
}

#[test]
fn protected_tag_pattern_matches() {
    let rule = ProtectedRefRule::default_phase1_rules()
        .pop()
        .unwrap_or_else(|| panic!("missing tag rule"));
    let change = RefChange {
        actor: "alice".to_string(),
        ref_name: "refs/tags/v1.0.0".to_string(),
        old_oid: "abc".to_string(),
        new_oid: "0000000000000000000000000000000000000000".to_string(),
        operation: RefOperation::Delete,
        force: false,
    };
    assert!(rule.evaluate(&change).is_err());
}
