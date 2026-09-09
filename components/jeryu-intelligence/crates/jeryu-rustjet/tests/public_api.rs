mod common;

use jeryu_rustjet::PublicApiDetector;

#[test]
fn detects_public_api_surface_symbols() {
    let graph = common::graph();
    let package = graph.package("core").expect("core package");
    let change = PublicApiDetector::new()
        .detect(package, "src/lib.rs")
        .expect("public api change");
    assert!(change.symbols.contains(&"CoreApi".to_string()));
    assert!(change.symbols.contains(&"public_core_value".to_string()));
}

#[test]
fn ignores_private_internal_file_without_pub_surface() {
    let graph = common::graph();
    let package = graph.package("core").expect("core package");
    let change = PublicApiDetector::new().detect(package, "src/internal.rs");
    assert!(change.is_none());
}
