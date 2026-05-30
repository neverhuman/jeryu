use crate::pathset::normalize_slashes;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChangedPath {
    pub path: String,
}

impl ChangedPath {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: normalize_slashes(path),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    paths: Vec<ChangedPath>,
}

impl ChangeSet {
    pub fn new(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Self {
        let mut normalized: Vec<_> = paths.into_iter().map(ChangedPath::new).collect();
        normalized.sort();
        normalized.dedup();
        Self { paths: normalized }
    }

    pub fn from_strings(paths: impl IntoIterator<Item = String>) -> Self {
        Self::new(paths)
    }

    pub fn paths(&self) -> &[ChangedPath] {
        &self.paths
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}
