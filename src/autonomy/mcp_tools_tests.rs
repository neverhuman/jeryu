use super::*;

#[test]
fn full_catalog_has_9_tools() {
    let d = descriptors();
    assert_eq!(
        d.len(),
        9,
        "Phase 9 tool count should be exactly 9 (6 read-only + 3 mutating)"
    );
}

#[test]
fn read_only_and_mutating_partition_the_catalog() {
    let ro = read_only();
    let mu = mutating();
    assert_eq!(ro.len(), 6);
    assert_eq!(mu.len(), 3);
    for d in &mu {
        assert!(
            d.requires_lease,
            "mutating tool {} must require_lease",
            d.name
        );
    }
    for d in &ro {
        assert!(
            !d.requires_lease,
            "read-only tool {} must not require_lease",
            d.name
        );
    }
}

#[test]
fn all_descriptors_use_vibegate_prefix() {
    for d in descriptors() {
        assert!(
            d.name.starts_with("vibegate."),
            "tool {} must use vibegate. prefix",
            d.name
        );
    }
}

#[test]
fn input_schemas_have_valid_json() {
    for d in descriptors() {
        assert!(
            d.input_schema.is_object(),
            "tool {} schema must be object",
            d.name
        );
        assert_eq!(
            d.input_schema["type"].as_str(),
            Some("object"),
            "tool {} schema.type must be 'object'",
            d.name
        );
    }
}

#[test]
fn descriptors_round_trip_through_serde() {
    let d = descriptors();
    let json = serde_json::to_string(&d).unwrap();
    let back: Vec<ToolDescriptor> = serde_json::from_str(&json).unwrap();
    assert_eq!(d.len(), back.len());
    for (a, b) in d.iter().zip(back.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.category, b.category);
        assert_eq!(a.requires_lease, b.requires_lease);
    }
}
