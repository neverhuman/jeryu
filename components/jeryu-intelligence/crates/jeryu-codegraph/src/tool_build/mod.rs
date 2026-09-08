//! Fast repeated-code clustering for Jankurai tool-building leads.
//!
//! The v1 scanner walks source files, builds identifier/literal-normalized
//! line windows, hashes them, and ranks repeated windows into tool-building
//! opportunity clusters. The v2 system scan layered on top adds: parallel
//! multi-repo scanning with progress callbacks, `.gitignore`-aware file
//! discovery, generated-zone and scaffold awareness, window quality filters,
//! overlap merging into maximal duplicated spans, category labelling, and
//! second-tier pattern families.
//!
//! Compatibility contract: [`scan_tool_build_clusters`] and
//! [`scan_tool_build_family`] preserve v1 output byte-for-byte (same walker,
//! same filters, same fingerprints), so persisted cluster ids and ignore
//! feedback stay valid. All new behavior is reached through
//! [`scan_tool_build_system`] / [`ToolBuildScanOptions`].

pub mod enrich;
mod families;
mod merge;
mod normalize;
mod progress;
mod scan;
mod walk;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{CodeGraphError, Result};

pub use families::group_pattern_families;
pub use progress::{ToolBuildScanPhase, ToolBuildScanProgress};

/// Runtime knobs for the fast tool-building scan.
///
/// FROZEN SHAPE: downstream repos construct this struct with exhaustive field
/// literals (no `..Default::default()`), so adding a field here breaks them at
/// the next pin bump. New knobs belong on [`ToolBuildScanOptions`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanConfig {
    /// Number of normalized non-empty lines in each fingerprinted window.
    pub window_lines: usize,
    /// Minimum normalized tokens in a window before it can form a cluster.
    pub min_normalized_tokens: usize,
    /// Minimum occurrences required for a cluster.
    pub min_occurrences: usize,
    /// Max bytes to read per source file; larger files are skipped.
    pub max_file_bytes: u64,
    /// Max ranked clusters returned to callers.
    pub max_clusters: usize,
    /// Minimum distinct repos a cluster must span to be returned. The single-root
    /// scan leaves this at `1`; the family scan raises it to `2` to surface only
    /// windows that repeat across more than one repo.
    #[serde(default = "default_min_repo_count")]
    pub min_repo_count: usize,
}

fn default_min_repo_count() -> usize {
    1
}

impl Default for ToolBuildScanConfig {
    fn default() -> Self {
        Self {
            window_lines: 8,
            min_normalized_tokens: 36,
            min_occurrences: 2,
            max_file_bytes: 512 * 1024,
            max_clusters: 50,
            min_repo_count: 1,
        }
    }
}

/// Extended knobs for the v2 system scan. Wraps the frozen
/// [`ToolBuildScanConfig`] so existing constructors never change shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanOptions {
    /// The frozen v1 knob set (window size, token floor, caps).
    pub base: ToolBuildScanConfig,
    /// Chain +1-shifted window clusters into maximal duplicated spans.
    pub merge_overlaps: bool,
    /// Minimum `call:`/`macro:`/`member:` anchor tokens per non-config window.
    pub min_anchor_tokens: usize,
    /// Minimum distinct normalized tokens per window (kills brace noise).
    pub min_distinct_tokens: usize,
    /// Skip windows whose import/use line fraction exceeds this (x100).
    pub max_import_fraction_x100: usize,
    /// Occurrence fraction (x100) above which a cluster is a test-pattern.
    pub test_fraction_threshold_x100: usize,
    /// Occurrence fraction (x100) above which a cluster is managed scaffold.
    pub scaffold_fraction_threshold_x100: usize,
    /// Honor each repo's `agent/generated-zones.toml` skip globs.
    pub honor_generated_zones: bool,
    /// Prefer `git ls-files` (gitignore-aware) discovery over the raw walker.
    pub use_git_ls_files: bool,
    /// Skip a repo's top-level directory entirely when it holds more than
    /// this many scannable files (0 = unlimited). Hand-written product code
    /// never reaches corpus scale in one directory; training corpora,
    /// scraped datasets, and research dumps do — and they would both
    /// serialize the scan and pollute clusters with non-product noise.
    /// Skipped files are counted, never silently dropped.
    #[serde(default)]
    pub max_files_per_top_dir: usize,
    /// Worker threads; `0` selects `std::thread::available_parallelism()`.
    pub threads: usize,
    /// Internal: byte-exact v1 behavior (v1 walker, no filters, no merging,
    /// `window_lines`-based line accounting). Set only by the compat wrappers.
    #[serde(default)]
    pub compat_v1: bool,
}

impl ToolBuildScanOptions {
    /// Defaults for the cross-repo system scan: only windows spanning 2+ repos,
    /// a larger cluster cap, every false-positive lane enabled.
    #[must_use]
    pub fn system_default() -> Self {
        Self {
            base: ToolBuildScanConfig {
                min_repo_count: 2,
                max_clusters: 200,
                ..ToolBuildScanConfig::default()
            },
            merge_overlaps: true,
            min_anchor_tokens: 3,
            min_distinct_tokens: 12,
            max_import_fraction_x100: 50,
            test_fraction_threshold_x100: 60,
            scaffold_fraction_threshold_x100: 60,
            honor_generated_zones: true,
            use_git_ls_files: true,
            max_files_per_top_dir: 4000,
            threads: 0,
            compat_v1: false,
        }
    }

    /// Byte-exact v1 behavior carrying a caller-supplied frozen config.
    #[must_use]
    pub fn v1_compat(base: ToolBuildScanConfig) -> Self {
        Self {
            base,
            merge_overlaps: false,
            min_anchor_tokens: 0,
            min_distinct_tokens: 0,
            max_import_fraction_x100: 100,
            test_fraction_threshold_x100: 101,
            scaffold_fraction_threshold_x100: 101,
            honor_generated_zones: false,
            use_git_ls_files: false,
            max_files_per_top_dir: 0,
            threads: 0,
            compat_v1: true,
        }
    }
}

/// Cluster category separating real tool candidates from intentional
/// duplication (managed scaffold), config echoes, and test-only repetition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolBuildCategory {
    /// Repeated product/application code worth extracting into a shared tool.
    #[default]
    ToolCandidate,
    /// Files propagated on purpose by the family scaffold renderer; already
    /// centrally governed, never a tool-build lead.
    ManagedScaffold,
    /// Repetition dominated by toml/yaml/json configuration.
    ConfigPattern,
    /// Repetition that lives (almost) entirely in test code.
    TestPattern,
}

impl ToolBuildCategory {
    /// Stable kebab-case label used in storage and APIs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToolCandidate => "tool-candidate",
            Self::ManagedScaffold => "managed-scaffold",
            Self::ConfigPattern => "config-pattern",
            Self::TestPattern => "test-pattern",
        }
    }

    /// Parse the stable kebab-case label; unknown labels become the default.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        match label {
            "managed-scaffold" => Self::ManagedScaffold,
            "config-pattern" => Self::ConfigPattern,
            "test-pattern" => Self::TestPattern,
            _ => Self::ToolCandidate,
        }
    }
}

/// Result of a tool-building scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanReport {
    /// Stable repository id for the scanned root.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// Root that was scanned.
    pub root: String,
    /// Scan timestamp in unix milliseconds.
    pub scanned_at: String,
    /// Files considered by the scanner.
    pub scanned_files: usize,
    /// Files skipped by size, binary decode, or excluded path.
    pub skipped_files: usize,
    /// Ranked repeated-code clusters.
    pub clusters: Vec<ToolBuildCluster>,
    /// Second-tier pattern families over the ranked clusters (system scan only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub families: Vec<ToolBuildClusterFamily>,
}

/// Ranked repeated-code cluster that can become a Jankurai tool-building task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildCluster {
    /// Stable cluster id derived from the normalized fingerprint.
    pub cluster_id: String,
    /// Stable repository id for the scan that produced the cluster.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// BLAKE3 fingerprint of the normalized window.
    pub fingerprint: String,
    /// Heuristic score for ranking possible tool-building opportunities.
    pub score: u64,
    /// Number of occurrences in the cluster.
    pub occurrence_count: usize,
    /// Number of distinct repos represented by the cluster.
    pub repo_count: usize,
    /// Number of distinct files represented by the cluster.
    pub file_count: usize,
    /// Total duplicated source lines covered by all occurrences.
    pub total_lines: usize,
    /// Dominant language label.
    pub language: String,
    /// Short deterministic insight for polling clients.
    pub insight: String,
    /// Normalized preview used for explanation and future AI review.
    pub normalized_preview: String,
    /// Cluster category (tool candidate vs scaffold/config/test repetition).
    #[serde(default)]
    pub category: ToolBuildCategory,
    /// Window cluster ids folded into this maximal cluster by overlap merging.
    /// Empty for unmerged clusters. Lets ignore feedback on old window ids
    /// propagate onto the merged cluster.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_cluster_ids: Vec<String>,
    /// Representative occurrences, capped for compact MCP responses.
    pub occurrences: Vec<ToolBuildOccurrence>,
    /// Optional ignore feedback from the durable store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignored: Option<ToolBuildIgnore>,
}

/// One code occurrence in a repeated-code cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildOccurrence {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// Repo-relative path.
    pub path: String,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// Language/domain label.
    pub language: String,
    /// Normalized-token count for the occurrence window.
    pub normalized_token_count: usize,
    /// Whether the occurrence lives under a test path.
    #[serde(default)]
    pub is_test: bool,
}

/// Durable ignore feedback for a cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildIgnore {
    /// Ignored cluster id.
    pub cluster_id: String,
    /// Human/agent reason the cluster should not produce tool-building work.
    pub reason: String,
    /// Actor that recorded the feedback.
    pub ignored_by: String,
    /// Feedback timestamp in unix milliseconds.
    pub ignored_at: String,
}

/// Second-tier grouping: clusters whose anchor signatures overlap are one
/// repeated *pattern* even when their exact normalized windows differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildClusterFamily {
    /// Stable id derived from (language, category, union anchor signature).
    pub family_id: String,
    /// Short human label mined from the most frequent shared anchors.
    pub label: String,
    /// Dominant language of the member clusters.
    pub language: String,
    /// Category shared by every member cluster.
    pub category: ToolBuildCategory,
    /// Member cluster ids, sorted.
    pub cluster_ids: Vec<String>,
    /// Sorted union of repos the member occurrences live in.
    pub repo_ids: Vec<String>,
    /// Number of member clusters.
    pub cluster_count: usize,
    /// Total occurrences across member clusters.
    pub occurrence_total: usize,
    /// Total distinct files across member clusters.
    pub file_total: usize,
    /// Sum of the members' anticipated LOC saved (see [`enrich`]).
    pub anticipated_loc_saved_total: usize,
    /// Sum of the members' heuristic scores.
    pub score_total: u64,
}

/// Scan a repository root and return ranked repeated-code clusters.
///
/// Byte-compatible with the v1 scanner: same walker, filters, fingerprints,
/// and ranking, so persisted cluster ids and ignore feedback stay valid.
pub fn scan_tool_build_clusters(
    root: impl AsRef<Path>,
    repo_id: impl Into<String>,
    commit_sha: impl Into<String>,
    config: ToolBuildScanConfig,
) -> Result<ToolBuildScanReport> {
    let root = root.as_ref();
    let repo_id = repo_id.into();
    let commit_sha = commit_sha.into();
    let options = ToolBuildScanOptions::v1_compat(config);
    let roots = vec![(repo_id.clone(), root.to_path_buf())];
    let mut report = scan::scan_roots(&roots, &repo_id, &commit_sha, &options, &|_| {})?;
    report.root = root.display().to_string();
    Ok(report)
}

/// Scan every `(repo_id, root)` pair into one shared fingerprint index so a
/// normalized window that appears in more than one repo collapses into a single
/// cross-repo cluster (`repo_count >= 2`). This is the tool-finder hot path:
/// set `config.min_repo_count = 2` to keep only windows that repeat across
/// repos — the leads worth extracting into a shared tool. The cluster rows are
/// labelled with `family_repo_id` (e.g. `family/jeryu-split`) while each
/// occurrence keeps its own `repo_id`, so a dossier can point at exact files.
pub fn scan_tool_build_family(
    roots: &[(String, PathBuf)],
    family_repo_id: impl Into<String>,
    commit_sha: impl Into<String>,
    config: ToolBuildScanConfig,
) -> Result<ToolBuildScanReport> {
    let family_repo_id = family_repo_id.into();
    let commit_sha = commit_sha.into();
    let options = ToolBuildScanOptions::v1_compat(config);
    let mut report = scan::scan_roots(roots, &family_repo_id, &commit_sha, &options, &|_| {})?;
    report.root = roots
        .iter()
        .map(|(repo_id, _)| repo_id.clone())
        .collect::<Vec<_>>()
        .join(",");
    Ok(report)
}

/// Scan every `(repo_id, root)` pair with the full v2 pipeline: parallel
/// workers, gitignore-aware discovery, generated-zone/scaffold/test awareness,
/// window quality filters, overlap merging, categories, and pattern families.
/// `on_progress` receives phase/counter events suitable for live streaming.
pub fn scan_tool_build_system(
    roots: &[(String, PathBuf)],
    system_repo_id: impl Into<String>,
    commit_label: impl Into<String>,
    options: &ToolBuildScanOptions,
    on_progress: &(dyn Fn(ToolBuildScanProgress) + Send + Sync),
) -> Result<ToolBuildScanReport> {
    let system_repo_id = system_repo_id.into();
    let commit_label = commit_label.into();
    let mut report = scan::scan_roots(roots, &system_repo_id, &commit_label, options, on_progress)?;
    report.root = roots
        .iter()
        .map(|(repo_id, _)| repo_id.clone())
        .collect::<Vec<_>>()
        .join(",");
    Ok(report)
}

/// Discover every repo across every split family under the given parents.
///
/// Looks for `parent/*-split/repos.manifest.toml` and (one level deeper, for
/// families whose manifest lives inside the namesake repo)
/// `parent/*-split/*/repos.manifest.toml`. Manifest `[[repo]]` entries may
/// carry an absolute `path`; entries without one resolve to a sibling
/// directory named after the repo.
///
/// Dedupe runs on LOGICAL identity (the `github_slug`/`github` remote, falling
/// back to the repo name), not just canonical path: family manifests routinely
/// point at extra live checkouts of the same repo (deploy clones, worktrees),
/// and scanning two checkouts of one repo manufactures fake "cross-repo"
/// duplication. First manifest in sorted order wins, so the family-canonical
/// checkout beats stray deployment clones. Returned sorted by repo id.
pub fn discover_system_repo_roots(manifest_parents: &[PathBuf]) -> Result<Vec<(String, PathBuf)>> {
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Deserialize)]
    struct LooseManifest {
        #[serde(default)]
        repo: Vec<LooseManifestRepo>,
    }
    #[derive(Deserialize)]
    struct LooseManifestRepo {
        path: Option<String>,
        name: Option<String>,
        github_slug: Option<String>,
        github: Option<String>,
    }

    impl LooseManifestRepo {
        /// Stable logical identity: explicit slug, else owner/name parsed from
        /// the github remote URL, else the bare repo name.
        fn logical_id(&self) -> Option<String> {
            if let Some(slug) = &self.github_slug {
                return Some(slug.to_ascii_lowercase());
            }
            if let Some(url) = &self.github {
                let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
                let mut segments = trimmed.rsplit('/');
                let name = segments.next()?;
                let owner = segments.next()?;
                let owner = owner.rsplit(':').next().unwrap_or(owner);
                return Some(format!("{owner}/{name}").to_ascii_lowercase());
            }
            self.name.as_ref().map(|name| name.to_ascii_lowercase())
        }
    }

    let mut manifests: BTreeSet<PathBuf> = BTreeSet::new();
    for parent in manifest_parents {
        let Ok(entries) = std::fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let family_dir = entry.path();
            if !family_dir.is_dir()
                || !family_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-split"))
            {
                continue;
            }
            let direct = family_dir.join("repos.manifest.toml");
            if direct.is_file() {
                manifests.insert(direct);
            }
            // Nested form: the manifest lives inside the namesake repo
            // (e.g. jankurai-split/jankurai/repos.manifest.toml).
            let Ok(children) = std::fs::read_dir(&family_dir) else {
                continue;
            };
            for child in children.flatten() {
                let nested = child.path().join("repos.manifest.toml");
                if child.path().is_dir() && nested.is_file() {
                    manifests.insert(nested);
                }
            }
        }
    }

    let mut seen_roots: BTreeSet<PathBuf> = BTreeSet::new();
    let mut seen_logical: BTreeSet<String> = BTreeSet::new();
    let mut roots: BTreeMap<String, PathBuf> = BTreeMap::new();
    for manifest in &manifests {
        let text = std::fs::read_to_string(manifest).map_err(|source| CodeGraphError::Index {
            path: manifest.display().to_string(),
            source,
        })?;
        let Ok(parsed) = toml::from_str::<LooseManifest>(&text) else {
            // A manifest that does not parse as the loose shape is skipped, not
            // fatal: system discovery must tolerate schema drift per family.
            continue;
        };
        let manifest_dir = manifest.parent().unwrap_or(Path::new("/"));
        let family_label = family_label_for(manifest_dir);
        for repo in parsed.repo {
            let resolved =
                resolve_repo_root(manifest_dir, repo.path.as_deref(), repo.name.as_deref());
            let Some(root) = resolved else { continue };
            let Ok(canonical) = root.canonicalize() else {
                continue;
            };
            if let Some(logical) = repo.logical_id()
                && !seen_logical.insert(logical)
            {
                // A second checkout of an already-claimed logical repo.
                continue;
            }
            if !seen_roots.insert(canonical.clone()) {
                continue;
            }
            let name = repo
                .name
                .clone()
                .or_else(|| {
                    canonical
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "repo".to_string());
            // Qualify with the family dir on a cross-family name collision.
            let repo_id = if roots.contains_key(&name) {
                format!("{family_label}/{name}")
            } else {
                name
            };
            roots.insert(repo_id, canonical);
        }
    }
    Ok(roots.into_iter().collect())
}

/// The `*-split` family directory name a manifest belongs to.
fn family_label_for(manifest_dir: &Path) -> String {
    let mut dir = Some(manifest_dir);
    while let Some(current) = dir {
        if current
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with("-split"))
        {
            return current
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("family")
                .to_string();
        }
        dir = current.parent();
    }
    "family".to_string()
}

/// Resolve a manifest repo entry to an on-disk root: explicit `path` when it
/// exists, else a sibling of the manifest named after the repo, else (for
/// nested manifests) a sibling of the manifest's parent directory.
fn resolve_repo_root(
    manifest_dir: &Path,
    path: Option<&str>,
    name: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = path {
        let candidate = PathBuf::from(path);
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    let name = name?;
    let sibling = manifest_dir.join(name);
    if sibling.is_dir() {
        return Some(sibling);
    }
    let parent_sibling = manifest_dir.parent()?.join(name);
    if parent_sibling.is_dir() {
        return Some(parent_sibling);
    }
    None
}

pub(crate) fn epoch_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
