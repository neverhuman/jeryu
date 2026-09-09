use crate::error::{RustJetError, RustJetResult};
use crate::pathset::{join, normalize_relative};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub type PackageId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePackage {
    pub name: PackageId,
    pub manifest_path: PathBuf,
    pub root: PathBuf,
    pub relative_root: String,
    pub dependencies: BTreeSet<PackageId>,
    pub dev_dependencies: BTreeSet<PackageId>,
    pub build_dependencies: BTreeSet<PackageId>,
    pub features: BTreeSet<String>,
    pub has_build_script: bool,
    pub is_proc_macro: bool,
    pub links_native_library: bool,
}

impl WorkspacePackage {
    pub fn all_declared_dependencies(&self) -> BTreeSet<PackageId> {
        self.dependencies
            .iter()
            .chain(self.dev_dependencies.iter())
            .chain(self.build_dependencies.iter())
            .cloned()
            .collect()
    }

    pub fn contains_relative_path(&self, changed_path: &str) -> bool {
        if self.relative_root == "." {
            return true;
        }
        changed_path == self.relative_root
            || changed_path.starts_with(&format!("{}/", self.relative_root))
    }

    pub fn path_inside_package<'a>(&self, changed_path: &'a str) -> Option<&'a str> {
        if self.relative_root == "." {
            return Some(changed_path);
        }
        changed_path.strip_prefix(&format!("{}/", self.relative_root))
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceManifest {
    pub root: PathBuf,
    pub packages: BTreeMap<PackageId, WorkspacePackage>,
}

impl WorkspaceManifest {
    pub fn load(root: impl AsRef<Path>) -> RustJetResult<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join("Cargo.toml");
        if !manifest_path.exists() {
            return Err(RustJetError::MissingWorkspaceManifest(manifest_path));
        }
        let text = read_to_string(&manifest_path)?;
        let member_paths = parse_workspace_members(&text, &manifest_path)?;
        if member_paths.is_empty() {
            return Err(RustJetError::EmptyWorkspace);
        }

        let mut packages = BTreeMap::new();
        for member in member_paths {
            let member_root = join(&root, &member);
            let package_manifest = member_root.join("Cargo.toml");
            let package_text = read_to_string(&package_manifest)?;
            let mut package = parse_package_manifest(&package_text, &package_manifest)?;
            package.root = member_root;
            package.relative_root = normalize_relative(&root, &package.root);
            package.manifest_path = package_manifest;
            packages.insert(package.name.clone(), package);
        }

        let package_names: BTreeSet<_> = packages.keys().cloned().collect();
        for package in packages.values_mut() {
            package
                .dependencies
                .retain(|name| package_names.contains(name));
            package
                .dev_dependencies
                .retain(|name| package_names.contains(name));
            package
                .build_dependencies
                .retain(|name| package_names.contains(name));
        }

        Ok(Self { root, packages })
    }

    pub fn package_for_path(&self, changed_path: &str) -> Option<&WorkspacePackage> {
        self.packages
            .values()
            .filter(|package| package.contains_relative_path(changed_path))
            .max_by_key(|package| package.relative_root.len())
    }

    pub fn package_names(&self) -> BTreeSet<PackageId> {
        self.packages.keys().cloned().collect()
    }
}

fn read_to_string(path: &Path) -> RustJetResult<String> {
    fs::read_to_string(path).map_err(|source| RustJetError::io(path, source))
}

fn parse_workspace_members(text: &str, path: &Path) -> RustJetResult<Vec<String>> {
    let Some(array) = parse_array(text, "members") else {
        return Err(RustJetError::parse(
            path,
            "workspace members array not found",
        ));
    };
    Ok(array)
}

fn parse_package_manifest(text: &str, path: &Path) -> RustJetResult<WorkspacePackage> {
    let package_section = section_body(text, "package");
    let Some(name) = scalar_in_section(package_section, "name") else {
        return Err(RustJetError::parse(path, "package name not found"));
    };

    let build_value = scalar_in_section(package_section, "build");
    let has_build_script = build_value.is_some()
        || path
            .parent()
            .is_some_and(|root| root.join("build.rs").exists());
    let links_native_library = scalar_in_section(package_section, "links").is_some();
    let is_proc_macro = scalar_in_section(section_body(text, "lib"), "proc-macro")
        .is_some_and(|value| value == "true");

    let dependencies = dependency_names(section_body(text, "dependencies"));
    let dev_dependencies = dependency_names(section_body(text, "dev-dependencies"));
    let build_dependencies = dependency_names(section_body(text, "build-dependencies"));
    let features = feature_names(section_body(text, "features"));

    Ok(WorkspacePackage {
        name,
        manifest_path: PathBuf::new(),
        root: PathBuf::new(),
        relative_root: String::new(),
        dependencies,
        dev_dependencies,
        build_dependencies,
        features,
        has_build_script,
        is_proc_macro,
        links_native_library,
    })
}

fn section_body<'a>(text: &'a str, section: &str) -> &'a str {
    let header = format!("[{section}]");
    let Some(start) = text.find(&header) else {
        return "";
    };
    let body_start = start + header.len();
    let rest = &text[body_start..];
    let end = rest.find("\n[").unwrap_or(rest.len());
    &rest[..end]
}

fn scalar_in_section(section: &str, key: &str) -> Option<String> {
    section.lines().find_map(|raw| {
        let line = strip_comment(raw).trim();
        let (lhs, rhs) = line.split_once('=')?;
        if lhs.trim() != key {
            return None;
        }
        Some(unquote(rhs.trim()))
    })
}

fn dependency_names(section: &str) -> BTreeSet<String> {
    section
        .lines()
        .filter_map(|raw| {
            let line = strip_comment(raw).trim();
            if line.is_empty() || line.starts_with('[') || line.starts_with('{') {
                return None;
            }
            let (lhs, _) = line.split_once('=')?;
            let key = lhs.trim();
            (!key.is_empty()).then(|| key.to_string())
        })
        .collect()
}

fn feature_names(section: &str) -> BTreeSet<String> {
    section
        .lines()
        .filter_map(|raw| {
            let line = strip_comment(raw).trim();
            let (lhs, _) = line.split_once('=')?;
            let key = lhs.trim();
            (!key.is_empty()).then(|| key.to_string())
        })
        .collect()
}

fn parse_array(text: &str, key: &str) -> Option<Vec<String>> {
    let pattern = format!("{key} = [");
    let start = text.find(&pattern)? + pattern.len();
    let tail = &text[start..];
    let end = tail.find(']')?;
    let values = tail[..end]
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(unquote)
        .filter(|value| !value.is_empty())
        .collect();
    Some(values)
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(',')
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiline_workspace_members() {
        let text = r#"
[workspace]
members = [
  "crates/a",
  "crates/b",
]
"#;
        let members = parse_workspace_members(text, Path::new("Cargo.toml")).unwrap();
        assert_eq!(members, vec!["crates/a", "crates/b"]);
    }
}
