use super::*;

#[test]
fn component_and_mirror_forms_keep_actual_source_and_logical_policy_distinct() {
    let mut fixture = Fixture::new();
    fixture.expected.identity.repository = "neverhuman/jeryu-cache".into();
    fixture.expected.identity.source_repository = "neverhuman/jeryu".into();
    fixture.expected.identity.scope = Scope::Component {
        path: "components/jeryu-cache".into(),
        tree: "e".repeat(40),
    };
    fixture.expected.identity.minimum = 91;
    fixture.expected.identity.max_soft = None;
    fixture.policy =
        b"workspace='jeryu-cache'\nminimum_score=91\nhard_findings_allowed=0\n".to_vec();
    fixture.expected.identity.candidate_policy_sha256 = audit_evidence::hash(&fixture.policy);
    fixture.expected.identity.governing_policy_sha256 = audit_evidence::hash(&fixture.policy);
    fixture.change_report(|report| {
        report["policy"]["minimum_score"] = json!(91);
        report["decision"]["minimum_score"] = json!(91);
    });
    let component = fixture.prepare();
    state(&component, "PENDING");
    assert!(
        component
            .destination()
            .starts_with("neverhuman/jeryu/component/jeryu-cache/")
    );
    assert_eq!(
        component.metadata()["expected_observation"]["identity"]["source_repository"],
        "neverhuman/jeryu"
    );
    let display: Value = serde_json::from_slice(&component.files["renderer-input.json"]).unwrap();
    assert_eq!(display["repository"], "neverhuman/jeryu-cache");
    assert_eq!(display["source_repository"], "neverhuman/jeryu");
    for wrong in ["neverhuman/jeryu-cache", "neverhuman/jeryu-tool"] {
        let mut expected = fixture.expected.clone();
        expected.identity.source_repository = wrong.into();
        assert!(
            prepare(
                &fixture.source,
                &expected,
                fixture.inputs(&fixture.receipt())
            )
            .is_err()
        );
    }
    let mut wrong_path = fixture.expected.clone();
    wrong_path.identity.scope = Scope::Component {
        path: "components/jeryu-tool".into(),
        tree: "e".repeat(40),
    };
    assert!(
        prepare(
            &fixture.source,
            &wrong_path,
            fixture.inputs(&fixture.receipt())
        )
        .is_err()
    );
    let component_receipt = fixture.receipt();
    fixture.expected.identity.scope = Scope::Standalone {
        originating_monorepo_commit: "a".repeat(40),
        export_provenance_sha256: "f".repeat(64),
    };
    // A standalone mirror cannot relabel the root repository's commit as its own.
    assert!(
        prepare(
            &fixture.source,
            &fixture.expected,
            fixture.inputs(&component_receipt)
        )
        .is_err()
    );
    fixture.expected.identity.source_repository = "neverhuman/jeryu-cache".into();
    fixture.expected.identity.source_commit = "f".repeat(40);
    fixture.change_report(|report| report["git"]["head"] = json!("ffffffff"));
    let mirror = fixture.prepare();
    state(&mirror, "PENDING");
    assert!(
        mirror
            .destination()
            .starts_with("neverhuman/jeryu-cache/standalone/jeryu-cache/")
    );
    state(
        &prepare(
            &fixture.source,
            &fixture.expected,
            fixture.inputs(&component_receipt),
        )
        .unwrap(),
        "ERROR",
    );
    assert_ne!(mirror.destination(), component.destination());
}

#[test]
fn ordinary_repository_and_dependency_forms_cannot_claim_another_source_repository() {
    let fixture = Fixture::new();
    for scope in [Scope::Repository, Scope::Dependency, Scope::Optional] {
        let mut expected = fixture.expected.clone();
        expected.identity.scope = scope;
        expected.identity.source_repository = "neverhuman/jeryu-tool".into();
        assert!(
            prepare(
                &fixture.source,
                &expected,
                fixture.inputs(&fixture.receipt())
            )
            .is_err()
        );
    }
    let mut receipt: Value = serde_json::from_slice(&fixture.receipt()).unwrap();
    receipt["identity"]
        .as_object_mut()
        .unwrap()
        .remove("source_repository");
    state(
        &prepare(
            &fixture.source,
            &fixture.expected,
            fixture.inputs(&serde_json::to_vec(&receipt).unwrap()),
        )
        .unwrap(),
        "ERROR",
    );
    let arguments = super::disk::arguments(&fixture, false, false);
    let context_path = fixture.temporary.join("context");
    let mut context: Value = serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context["identity"]
        .as_object_mut()
        .unwrap()
        .remove("source_repository");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    assert!(cli::run(&fixture.source, arguments).is_err());
    assert_eq!(fs::read_dir(&fixture.output).unwrap().count(), 0);
}
