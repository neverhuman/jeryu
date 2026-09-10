use super::*;

#[test]
fn versioned_enrollment_reports_both_optional_versions_without_duplicate_error() {
    let mut sources = enrollment(COMMIT, false, 95);
    sources["schema"] = json!("jeryu.audit-repositories/v2");
    sources["sources"][0]["scope"] = json!("optional");
    let mut second = sources["sources"][0].clone();
    second["commit"] = json!(OTHER);
    second["minimum"] = json!(85);
    sources["sources"].as_array_mut().unwrap().push(second);
    let mut inventory = Inventory::default();
    inventory.cargo_lock("first/Cargo.lock", &locked_git(COMMIT), &[IMAGE]);
    inventory.cargo_lock("second/Cargo.lock", &locked_git(OTHER), &[IMAGE]);
    inventory.compare_enrollment(&sources);
    let report = inventory.finish().unwrap();
    assert!(report["errors"].as_array().unwrap().is_empty());
    let rows = report["enrollment"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["status"] == "enrolled")
            .count(),
        2
    );
    for commit in [COMMIT, OTHER] {
        assert!(
            rows.iter()
                .any(|row| row["commit"] == commit && row["status"] == "enrolled")
        );
    }
}

#[test]
fn versioned_enrollment_rejects_duplicates_and_missing_pins_without_borrowing_heads() {
    for invalid in [
        Value::Null,
        json!("main"),
        json!("ABCDEF0123456789abcdef0123456789abcdef0123"),
    ] {
        let mut sources = enrollment(COMMIT, false, 85);
        sources["schema"] = json!("jeryu.audit-repositories/v2");
        sources["sources"][0]["scope"] = json!("optional");
        sources["sources"][0]["commit"] = invalid;
        let mut inventory = Inventory::default();
        inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[IMAGE]);
        inventory.compare_enrollment(&sources);
        let report = inventory.finish().unwrap();
        assert!(has_reason(&report, "invalid_or_duplicate_enrollment"));
        assert!(has_reason(&report, "missing_or_wrong_revision"));
    }
    let mut sources = enrollment(COMMIT, false, 85);
    sources["schema"] = json!("jeryu.audit-repositories/v2");
    sources["sources"][0]["scope"] = json!("optional");
    let duplicate = sources["sources"][0].clone();
    sources["sources"].as_array_mut().unwrap().push(duplicate);
    let mut inventory = Inventory::default();
    inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[IMAGE]);
    inventory.compare_enrollment(&sources);
    assert!(has_reason(
        &inventory.finish().unwrap(),
        "invalid_or_duplicate_enrollment"
    ));

    sources["sources"].as_array_mut().unwrap().truncate(1);
    let mut inventory = Inventory::default();
    inventory.cargo_lock("Cargo.lock", &locked_git(OTHER), &[IMAGE]);
    inventory.compare_enrollment(&sources);
    let report = inventory.finish().unwrap();
    assert!(has_reason(&report, "missing_or_wrong_revision"));
    assert!(
        !report["enrollment"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["status"] == "enrolled")
    );
}

#[test]
fn owning_lock_binding_preserves_distinct_cargo_urls_without_inventing_a_patch() {
    let original = "https://github.com/neverhuman/support.git";
    let alternate = "https://www.github.com/neverhuman/support.git";
    assert_eq!(github_slug(original), github_slug(alternate));
    for declared in [original, alternate] {
        let manifest = json!({"package":{"name":"consumer"},"dependencies":{
            "support":{"git":declared,"rev":COMMIT}}});
        let mut lock = locked_git(COMMIT);
        lock["package"][0]["source"] = json!(format!("git+{alternate}?rev={COMMIT}#{COMMIT}"));
        let mut inventory = Inventory::default();
        inventory.cargo_manifest(
            "consumer/Cargo.toml",
            "Cargo.lock",
            &manifest,
            &Value::Null,
            &[IMAGE],
        );
        inventory.cargo_lock("Cargo.lock", &lock, &[IMAGE]);
        let mut sources = enrollment(COMMIT, false, 85);
        sources["schema"] = json!("jeryu.audit-repositories/v2");
        sources["sources"][0]["scope"] = json!("optional");
        inventory.compare_enrollment(&sources);
        let report = inventory.finish().unwrap();
        assert_eq!(
            has_reason(&report, "git_declaration_not_bound_to_owning_lock_package"),
            declared != alternate
        );
        let declaration = report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "cargo-declaration")
            .unwrap();
        assert_eq!(declaration["facts"]["url"], declared);
        let locked = report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "cargo-locked-package")
            .unwrap();
        assert_eq!(locked["facts"]["url"], alternate);
        if declared == alternate {
            assert_eq!(declaration["facts"]["lock_binding"]["url"], alternate);
        } else {
            assert!(declaration["facts"].get("lock_binding").is_none());
        }
    }
}
