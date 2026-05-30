mod common;

#[test]
fn loads_fixture_workspace_graph() {
    let graph = common::graph();
    let names = graph.package_names();
    assert_eq!(names.len(), 5);
    assert!(names.contains("core"));
    assert!(names.contains("api"));
    assert!(names.contains("app"));
    assert!(names.contains("macro_magic"));
    assert!(names.contains("native"));
}

#[test]
fn computes_reverse_dependencies_transitively() {
    let graph = common::graph();
    let reverse = graph.transitive_reverse_dependencies_of("core");
    assert!(reverse.contains("api"));
    assert!(reverse.contains("app"));
}
