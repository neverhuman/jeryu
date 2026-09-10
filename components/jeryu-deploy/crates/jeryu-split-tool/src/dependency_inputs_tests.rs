use super::*;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "1123456789abcdef0123456789abcdef01234567";
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123";

fn enrollment(commit: &str, required: bool, minimum: u8) -> Value {
    json!({"schema":"jeryu.audit-repositories/v1","sources":[{
        "repository":"neverhuman/support","scope":"dependency","path":null,
        "commit":commit,"minimum":minimum,"required":required,"reason":null}]})
}
fn locked_git(commit: &str) -> Value {
    json!({"version":4,"package":[{"name":"support","version":"1.0.0",
        "source":format!("git+https://github.com/neverhuman/support.git?tag=v1#{commit}")}]})
}
fn pin() -> Value {
    json!({"jankurai":{"rev":COMMIT,"tag":"v1","source_tree":COMMIT,
        "source_archive_sha256":SHA,"cargo_lock_sha256":SHA,"binary_sha256":SHA,
        "vendor_files_sha256":SHA,"build_context_sha256":SHA},
        "distribution":{"source_repository":"https://github.com/neverhuman/support.git"},
        "floors":{"default":85,"public-portal":85,"jeryu-ci-runner":91,"jeryu-tool":85}})
}
fn has_reason(report: &Value, reason: &str) -> bool {
    report["unresolved"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["reason"] == reason)
}

#[test]
fn malformed_input_does_not_suppress_an_unrelated_structured_identity() {
    let mut inventory = Inventory::default();
    assert!(
        inventory
            .parse("Cargo.lock", "[[package]", "toml", REQUIRED)
            .is_none()
    );
    inventory.tool_manifest("tool-manifest.toml", &pin());
    let report = inventory.finish().unwrap();
    assert!(has_reason(&report, "invalid_structured_input"));
    assert!(
        report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "auditor-pin" && row["facts"]["commit"] == COMMIT)
    );
    assert_eq!(report["complete_for"][AUDIT]["complete"], false);
}

#[test]
fn exact_owned_pin_requires_the_matching_revision_requiredness_and_floor() {
    for (commit, required, minimum, expected) in [
        (COMMIT, true, 85, "enrolled"),
        (OTHER, true, 85, "missing_or_wrong_revision"),
        (COMMIT, false, 85, "requiredness_or_floor_mismatch"),
        (COMMIT, true, 84, "missing_or_wrong_revision"),
    ] {
        let mut inventory = Inventory::default();
        inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[AUDIT]);
        inventory.compare_enrollment(&enrollment(commit, required, minimum));
        let report = inventory.finish().unwrap();
        assert!(
            report["enrollment"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["status"] == expected)
        );
        assert_eq!(
            report["errors"].as_array().unwrap().is_empty(),
            expected == "enrolled"
        );
    }
    let mut stronger = Inventory::default();
    stronger.floors.insert("neverhuman/support".into(), 95);
    stronger.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[AUDIT]);
    stronger.compare_enrollment(&enrollment(COMMIT, true, 91));
    assert!(has_reason(
        &stronger.finish().unwrap(),
        "requiredness_or_floor_mismatch"
    ));
}

#[test]
fn optional_owned_drift_remains_separate_from_sqlite_audit_completeness() {
    let mut inventory = Inventory::default();
    inventory.cargo_lock("optional/Cargo.lock", &locked_git(COMMIT), &[REDLINE]);
    inventory.compare_enrollment(&enrollment(OTHER, false, 85));
    let report = inventory.finish().unwrap();
    assert_eq!(report["complete_for"][REDLINE]["complete"], false);
    assert_eq!(report["complete_for"][AUDIT]["complete"], true);
    assert!(report["errors"].as_array().unwrap().is_empty());
    assert!(
        report["enrollment"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["status"] == "declared_unconsumed")
    );
}

#[test]
fn git_declaration_cannot_supply_a_missing_or_different_lock_source() {
    let manifest = json!({"package":{"name":"consumer"},"dependencies":{
        "support":{"git":"https://github.com/neverhuman/support.git","rev":COMMIT}}});
    for locked in [None, Some(OTHER), Some(COMMIT)] {
        let mut inventory = Inventory::default();
        inventory.cargo_manifest(
            "consumer/Cargo.toml",
            "Cargo.lock",
            &manifest,
            &Value::Null,
            &[AUDIT],
        );
        if let Some(commit) = locked {
            inventory.cargo_lock("Cargo.lock", &locked_git(commit), &[AUDIT]);
        }
        inventory.compare_enrollment(&enrollment(COMMIT, true, 85));
        let report = inventory.finish().unwrap();
        assert_eq!(
            has_reason(&report, "git_declaration_not_bound_to_owning_lock_package"),
            locked != Some(COMMIT)
        );
    }
}

#[test]
fn underfloor_profiles_are_reported_without_changing_the_auditor_pin() {
    let mut old = pin();
    old["floors"] = json!({"default":85,"public-portal":75,"jeryu-ci-runner":80,"jeryu-tool":65});
    let mut inventory = Inventory::default();
    inventory.tool_manifest("tool-manifest.toml", &old);
    let report = inventory.finish().unwrap();
    assert_eq!(
        report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["reason"] == "audit_profile_below_effective_floor")
            .count(),
        3
    );
    assert!(
        report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "auditor-pin" && row["facts"]["commit"] == COMMIT)
    );
    for name in ["jeryu-cache", "jeryu-ci-runner", "jeryu-jira"] {
        assert_eq!(crate::audit_census::effective_floor(name), 91);
    }
}

#[test]
fn npm_workspace_names_do_not_invent_external_owners_but_git_sources_are_observed() {
    let local = BTreeSet::from(["web".into(), "ux-qa".into()]);
    let lock = json!({"lockfileVersion":3,"packages":{
        "":{},"ux-qa":{"name":"@jankurai/ux-qa","version":"1.0.0"},
        "node_modules/@jankurai/ux-qa":{"resolved":"ux-qa","link":true},
        "node_modules/support":{"version":"1.0.0",
            "resolved":format!("git+https://github.com/neverhuman/support.git#{COMMIT}")}}});
    let mut inventory = Inventory::default();
    inventory.npm_lock("package-lock.json", &lock, &local, &[AUDIT]);
    inventory.compare_enrollment(&enrollment(COMMIT, true, 85));
    let report = inventory.finish().unwrap();
    let rows = report["observations"].as_array().unwrap();
    assert!(
        rows.iter()
            .filter(|row| row["facts"]["source_kind"] == "workspace")
            .all(|row| row["facts"]["repository"].is_null())
    );
    assert!(
        rows.iter()
            .any(|row| row["facts"]["repository"] == "neverhuman/support"
                && row["facts"]["commit"] == COMMIT)
    );
    assert!(!has_reason(&report, "npm_integrity_missing"));
}

#[test]
fn literal_workflow_uses_are_separate_from_comments_script_bodies_and_expressions() {
    let source = format!(
        "# uses: neverhuman/fake@{COMMIT}\nsteps:\n  - uses: actions/checkout@{COMMIT}\n  - run: |\n      uses: neverhuman/script-text@{COMMIT}\n  - uses: ${{{{ inputs.action }}}}\n"
    );
    let mut inventory = Inventory::default();
    inventory.workflow(".github/workflows/ci.yml", &source, &[AUDIT]);
    let report = inventory.finish().unwrap();
    let rows = report["observations"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["facts"]["repository"], "actions/checkout");
    assert!(has_reason(&report, "dynamic_or_unsupported_workflow_use"));
    assert_eq!(report["complete_for"][AUDIT]["complete"], false);
}

#[test]
fn docker_stage_aliases_are_local_and_dynamic_inputs_remain_optional() {
    let source = format!(
        "# FROM neverhuman/fake:latest\nFROM rust@sha256:{SHA} AS build\nFROM build AS runtime\nARG JANKURAI_REV=\"{COMMIT}\"\nRUN npm install -g @jeryu/jekko-cli\nFROM ${{BASE}} AS extra\n"
    );
    let mut inventory = Inventory::default();
    inventory.dockerfile("images/agent-sandbox/Dockerfile", &source);
    let report = inventory.finish().unwrap();
    let rows = report["observations"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "container-image")
            .count(),
        1
    );
    assert!(
        rows.iter()
            .any(|row| row["facts"]["package"] == "@jeryu/jekko-cli"
                && row["facts"]["repository"].is_null())
    );
    assert!(has_reason(&report, "dynamic_image_base"));
    assert_eq!(report["complete_for"][IMAGE]["complete"], false);
    assert_eq!(report["complete_for"][AUDIT]["complete"], true);
}

#[test]
fn malformed_tool_rows_and_removed_checksums_do_not_hide_later_rows() {
    let mut inventory = Inventory::default();
    inventory.tsv("ci/tools.lock.tsv",&format!("bad\trow\nknown\t1\ttool\t{SHA}\thttps://github.com/thirdparty/tool/releases/download/v1/tool.tar\n"),true);
    inventory.cargo_lock(
        "Cargo.lock",
        &json!({"package":[{"name":"crate","version":"1",
        "source":"registry+https://github.com/rust-lang/crates.io-index"}]}),
        &[AUDIT],
    );
    let report = inventory.finish().unwrap();
    assert!(has_reason(&report, "invalid_tool_lock_row"));
    assert!(has_reason(&report, "registry_checksum_missing_or_invalid"));
    assert!(
        report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["facts"]["package"] == "known")
    );
}

#[test]
fn unsafe_members_and_invalid_enrollment_cannot_silently_satisfy_discovery() {
    let mut inventory = Inventory::default();
    assert_eq!(
        member_paths(
            &mut inventory,
            "Cargo.toml",
            &json!(["components/a", "../outside", "components/*", "components/a"]),
            REQUIRED
        ),
        vec!["components/a"]
    );
    let mut invalid = enrollment(COMMIT, true, 85);
    invalid["sources"][0]["unexpected"] = json!("field");
    inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[AUDIT]);
    inventory.compare_enrollment(&invalid);
    let report = inventory.finish().unwrap();
    assert!(has_reason(&report, "dynamic_or_escaping_workspace_member"));
    assert!(has_reason(&report, "duplicate_workspace_member"));
    assert!(has_reason(&report, "invalid_or_duplicate_enrollment"));
    assert!(has_reason(&report, "missing_or_wrong_revision"));
}

#[test]
fn deterministic_output_binds_changed_input_bytes_and_preserves_unknown_license() {
    let build = |reverse: bool, source: &str| {
        let mut inventory = Inventory::default();
        for path in if reverse { ["b", "a"] } else { ["a", "b"] } {
            inventory.inputs.insert(
                path.into(),
                json!({"path":path,"sha256":hash(source.as_bytes())}),
            );
        }
        inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[AUDIT]);
        inventory.finish().unwrap()
    };
    let first = build(false, "first");
    assert_eq!(first, build(true, "first"));
    assert_ne!(
        first["source_input_sha256"],
        build(false, "changed")["source_input_sha256"]
    );
    assert_eq!(
        first["observations"][0]["facts"]["license_status"],
        "not_supplied_by_input"
    );
    assert!(safe_url("https://user:secret@github.com/neverhuman/support").is_none());
    assert!(safe_url("https://github.com/neverhuman/support?token=secret").is_none());
}

#[test]
fn optional_lock_or_another_package_cannot_bind_a_required_declaration() {
    let manifest = json!({"package":{"name":"consumer"},"dependencies":{
        "support":{"git":"https://github.com/neverhuman/support.git","rev":COMMIT}}});
    for (lock_path, package) in [
        ("optional/Cargo.lock", "support"),
        ("Cargo.lock", "other-package"),
    ] {
        let mut inventory = Inventory::default();
        inventory.cargo_manifest(
            "consumer/Cargo.toml",
            "Cargo.lock",
            &manifest,
            &Value::Null,
            &[AUDIT],
        );
        let mut lock = locked_git(COMMIT);
        lock["package"][0]["name"] = json!(package);
        inventory.cargo_lock(lock_path, &lock, &[REDLINE]);
        inventory.compare_enrollment(&enrollment(COMMIT, false, 85));
        let report = inventory.finish().unwrap();
        assert!(has_reason(
            &report,
            "git_declaration_not_bound_to_owning_lock_package"
        ));
        assert_eq!(report["complete_for"][AUDIT]["complete"], false);
    }
    let mut inventory = Inventory::default();
    inventory.cargo_manifest(
        "consumer/Cargo.toml",
        "Cargo.lock",
        &manifest,
        &Value::Null,
        &[AUDIT],
    );
    inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[REDLINE]);
    inventory.compare_enrollment(&enrollment(COMMIT, false, 85));
    assert!(has_reason(
        &inventory.finish().unwrap(),
        "requiredness_or_floor_mismatch"
    ));
}

#[test]
fn wrong_audit_form_or_local_path_cannot_satisfy_an_external_dependency() {
    for (scope, path) in [
        ("bogus", Value::Null),
        ("component", json!("components/support")),
        ("dependency", json!("local-support")),
        ("optional", Value::Null),
    ] {
        let mut source = enrollment(COMMIT, true, 85);
        source["sources"][0]["scope"] = json!(scope);
        source["sources"][0]["path"] = path;
        let mut inventory = Inventory::default();
        inventory.cargo_lock("Cargo.lock", &locked_git(COMMIT), &[AUDIT]);
        inventory.compare_enrollment(&source);
        let report = inventory.finish().unwrap();
        assert!(
            !report["enrollment"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["status"] == "enrolled")
        );
        assert_eq!(report["complete_for"][AUDIT]["complete"], false);
    }
}

#[test]
fn rejected_npm_links_and_image_locators_do_not_publish_credentials() {
    let secret = "source-secret-sentinel";
    let mut inventory = Inventory::default();
    for package_path in ["node_modules/bad", "", "local-workspace"] {
        inventory.npm_lock("package-lock.json",&json!({"lockfileVersion":3,"packages":{
            package_path:{"link":true,"resolved":format!("https://user:{secret}@github.com/neverhuman/support")}}}),
            &BTreeSet::from(["local-workspace".into()]),&[AUDIT]);
    }
    inventory.dockerfile("images/agent-sandbox/Dockerfile",
        &format!("FROM https://user:{secret}@registry.invalid/image?token={secret}\nARG JANKURAI_BUILDER_IMAGE=\"https://user:{secret}@registry.invalid/image\"\nARG JANKURAI_IMAGE=rust@sha256:{SHA}\n"));
    let report = inventory.finish().unwrap();
    assert!(!report.to_string().contains(secret));
    assert!(has_reason(&report, "unresolved_npm_link"));
    assert!(has_reason(
        &report,
        "unsupported_or_sensitive_image_locator"
    ));
    let npm: Vec<_> = report["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "npm-locked-package")
        .collect();
    assert_eq!(npm.len(), 3);
    assert!(
        npm.iter()
            .all(|row| row["facts"]["workspace_target"].is_null())
    );
    assert!(
        report["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "image-auditor-argument"
                && row["facts"]["argument"] == "JANKURAI_IMAGE"
                && row["facts"]["value"] == format!("rust@sha256:{SHA}"))
    );
}

#[path = "dependency_inputs_version_tests.rs"]
mod version_tests;
