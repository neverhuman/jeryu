use crate::error::{RustJetError, RustJetResult};
use crate::manifest::{PackageId, WorkspaceManifest, WorkspacePackage};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct WorkspaceGraph {
    manifest: WorkspaceManifest,
    reverse_dependencies: BTreeMap<PackageId, BTreeSet<PackageId>>,
    direct_dependencies: BTreeMap<PackageId, BTreeSet<PackageId>>,
}

impl WorkspaceGraph {
    pub fn load(root: impl AsRef<Path>) -> RustJetResult<Self> {
        let manifest = WorkspaceManifest::load(root)?;
        Self::from_manifest(manifest)
    }

    pub fn from_manifest(manifest: WorkspaceManifest) -> RustJetResult<Self> {
        if manifest.packages.is_empty() {
            return Err(RustJetError::EmptyWorkspace);
        }

        let mut reverse_dependencies: BTreeMap<PackageId, BTreeSet<PackageId>> = BTreeMap::new();
        let mut direct_dependencies: BTreeMap<PackageId, BTreeSet<PackageId>> = BTreeMap::new();

        for name in manifest.packages.keys() {
            reverse_dependencies.entry(name.clone()).or_default();
            direct_dependencies.entry(name.clone()).or_default();
        }

        for (package_name, package) in &manifest.packages {
            let deps = package.all_declared_dependencies();
            direct_dependencies.insert(package_name.clone(), deps.clone());
            for dep in deps {
                reverse_dependencies
                    .entry(dep)
                    .or_default()
                    .insert(package_name.clone());
            }
        }

        Ok(Self {
            manifest,
            reverse_dependencies,
            direct_dependencies,
        })
    }

    pub fn manifest(&self) -> &WorkspaceManifest {
        &self.manifest
    }

    pub fn package(&self, name: &str) -> Option<&WorkspacePackage> {
        self.manifest.packages.get(name)
    }

    pub fn packages(&self) -> impl Iterator<Item = &WorkspacePackage> {
        self.manifest.packages.values()
    }

    pub fn package_names(&self) -> BTreeSet<PackageId> {
        self.manifest.package_names()
    }

    pub fn package_for_path(&self, changed_path: &str) -> Option<&WorkspacePackage> {
        self.manifest.package_for_path(changed_path)
    }

    pub fn direct_dependencies_of(&self, package: &str) -> BTreeSet<PackageId> {
        self.direct_dependencies
            .get(package)
            .cloned()
            .unwrap_or_default()
    }

    pub fn direct_reverse_dependencies_of(&self, package: &str) -> BTreeSet<PackageId> {
        self.reverse_dependencies
            .get(package)
            .cloned()
            .unwrap_or_default()
    }

    pub fn transitive_reverse_dependencies_of(&self, package: &str) -> BTreeSet<PackageId> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<_> = self
            .direct_reverse_dependencies_of(package)
            .into_iter()
            .collect();
        while let Some(next) = queue.pop_front() {
            if !seen.insert(next.clone()) {
                continue;
            }
            for dependent in self.direct_reverse_dependencies_of(&next) {
                queue.push_back(dependent);
            }
        }
        seen
    }

    pub fn consumers_of_proc_macro(&self, package: &str) -> BTreeSet<PackageId> {
        self.transitive_reverse_dependencies_of(package)
    }

    pub fn all_packages(&self) -> BTreeSet<PackageId> {
        self.package_names()
    }
}
