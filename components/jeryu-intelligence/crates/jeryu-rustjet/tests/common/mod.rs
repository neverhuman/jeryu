use jeryu_rustjet::WorkspaceGraph;
use std::path::PathBuf;

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-small")
}

pub fn graph() -> WorkspaceGraph {
    WorkspaceGraph::load(fixture_root()).expect("fixture graph loads")
}
