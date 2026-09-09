//! Manifest-backed split-family repository classification.

use super::*;

#[derive(Clone, Debug)]
pub(super) struct SplitCatalog {
    /// Keyed by lowercase slug. Both GitHub slugs (`neverhuman/jeryu`) and
    /// local forge slugs (`jeryu/jeryu`) are indexed because repos are
    /// registered locally under the forge owner.
    entries: BTreeMap<String, SplitCatalogEntry>,
}

#[derive(Clone, Debug)]
struct SplitCatalogEntry {
    family: String,
    role: RepositoryRole,
}

#[derive(Debug, Deserialize)]
struct SplitManifest {
    repo_family: Option<String>,
    repo: Option<Vec<SplitManifestRepo>>,
}

#[derive(Debug, Deserialize)]
struct SplitManifestRepo {
    name: Option<String>,
    github_slug: Option<String>,
    jeryu_slug: Option<String>,
    profile: Option<String>,
}

/// Role for a split-family repo. The tool control plane and its discovery arm
/// ride the same scripts/docs `public-portal` build profile as the real portal,
/// so role can't be read from the profile alone: the `-tool` / `-tool-finder`
/// names disambiguate (and generalize across families, e.g. `jekko-tool`).
fn role_for(name: Option<&str>, profile: Option<&str>) -> RepositoryRole {
    match name {
        Some(n) if n.ends_with("-tool-finder") => RepositoryRole::SplitMember,
        Some(n) if n.ends_with("-tool") => RepositoryRole::ToolControlPlane,
        _ if profile == Some("public-portal") => RepositoryRole::PublicPortal,
        _ => RepositoryRole::SplitMember,
    }
}

/// The canonical repo name: the manifest `name` if present, else the last
/// segment of a slug (so role classification works even without an explicit
/// name field).
fn repo_canonical_name(repo: &SplitManifestRepo) -> Option<String> {
    if let Some(name) = repo.name.as_ref().filter(|n| !n.trim().is_empty()) {
        return Some(name.clone());
    }
    repo.jeryu_slug
        .as_ref()
        .or(repo.github_slug.as_ref())
        .and_then(|slug| slug.rsplit('/').next())
        .map(str::to_string)
}

/// Resolve `jeryu-tool/tools-registry.toml` from the split manifest, which lives
/// at the split root. `None` when no manifest is wired, so the golden-box
/// endpoint reports an empty registry.
pub(super) fn resolve_tool_registry_path(manifests: &[PathBuf]) -> Option<PathBuf> {
    let manifest = manifests.first()?;
    let split_root = manifest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    Some(split_root.join("jeryu-tool").join("tools-registry.toml"))
}

impl SplitCatalog {
    pub(super) fn load(manifests: &[PathBuf]) -> Self {
        if manifests.is_empty() {
            return Self::builtin();
        }
        let mut catalog = Self::empty();
        for manifest in manifests {
            if let Some(loaded) = Self::from_manifest(manifest) {
                catalog.entries.extend(loaded.entries);
            }
        }
        if catalog.entries.is_empty() {
            Self::builtin()
        } else {
            catalog
        }
    }

    fn empty() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub(super) fn builtin() -> Self {
        let family = "jeryu-split".to_string();
        let mut catalog = Self::empty();
        for slug in ["neverhuman/jeryu", "jeryu/jeryu"] {
            catalog.insert(slug, &family, RepositoryRole::PublicPortal);
        }
        for slug in ["neverhuman/jeryu-tool", "jeryu/jeryu-tool"] {
            catalog.insert(slug, &family, RepositoryRole::ToolControlPlane);
        }
        for slug in [
            "neverhuman/jeryu-core",
            "neverhuman/jeryu-ci-runner",
            "neverhuman/jeryu-cache",
            "neverhuman/jeryu-intelligence",
            "neverhuman/jeryu-web",
            "neverhuman/jeryu-release-ops",
            "neverhuman/jeryu-deploy",
            "neverhuman/jeryu-tool-finder",
            "jeryu/jeryu-core",
            "jeryu/jeryu-ci-runner",
            "jeryu/jeryu-cache",
            "jeryu/jeryu-intelligence",
            "jeryu/jeryu-web",
            "jeryu/jeryu-release-ops",
            "jeryu/jeryu-deploy",
            "jeryu/jeryu-tool-finder",
        ] {
            catalog.insert(slug, &family, RepositoryRole::SplitMember);
        }
        catalog
    }

    fn from_manifest(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let manifest: SplitManifest = toml::from_str(&text).ok()?;
        let family = manifest
            .repo_family
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "jeryu-split".to_string());
        let mut catalog = Self::empty();
        for repo in manifest.repo.unwrap_or_default() {
            // Compute role before the slugs move github_slug/jeryu_slug out.
            let role = role_for(
                repo_canonical_name(&repo).as_deref(),
                repo.profile.as_deref(),
            );
            let slugs: Vec<String> = [repo.github_slug, repo.jeryu_slug]
                .into_iter()
                .flatten()
                .map(|slug| slug.to_ascii_lowercase())
                .collect();
            if slugs.is_empty() {
                continue;
            }
            for slug in slugs {
                catalog.insert(&slug, &family, role.clone());
            }
        }
        Some(catalog)
    }

    fn insert(&mut self, slug: &str, family: &str, role: RepositoryRole) {
        self.entries.insert(
            slug.to_ascii_lowercase(),
            SplitCatalogEntry {
                family: family.to_string(),
                role,
            },
        );
    }

    pub(super) fn classify(&self, owner: &str, name: &str) -> Option<(String, RepositoryRole)> {
        let slug = format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            name.to_ascii_lowercase()
        );
        self.entries
            .get(&slug)
            .map(|entry| (entry.family.clone(), entry.role.clone()))
    }
}
