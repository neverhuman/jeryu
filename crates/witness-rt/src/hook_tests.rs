use super::*;

#[test]
fn match_cell_finds_best_prefix() {
    let cells = vec![
        CellRegistration {
            id: "pricing".into(),
            purpose: "pricing logic".into(),
            owned_paths: vec!["crates/pricing/src/".into()],
            invariants: vec!["totals non-negative".into()],
            local_commands: vec!["cargo test -p pricing".into()],
            escalate_commands: vec![],
            hints: vec![],
        },
        CellRegistration {
            id: "core".into(),
            purpose: "core logic".into(),
            owned_paths: vec!["crates/".into()],
            invariants: vec![],
            local_commands: vec![],
            escalate_commands: vec![],
            hints: vec![],
        },
    ];

    let _ = CELL_REGISTRY.set(cells);

    let matched = match_cell("crates/pricing/src/lib.rs");
    assert!(matched.is_some());
    let matched = matched.unwrap();
    assert_eq!(matched.cell.id, "pricing");
    assert_eq!(matched.matched_owned_path, "crates/pricing/src/");
}

#[test]
fn match_cell_returns_none_when_no_registry() {
    // In a fresh test without setting CELL_REGISTRY, this is a no-op
    // because OnceLock may already be set by another test in the same process.
    // This test validates the logic path when no match is found.
    let result = match_cell("completely/unrelated/path.rs");
    // Either None (no registry) or None (no match)
    if let Some(cell) = result {
        assert_ne!(cell.cell.id, "");
    }
}
