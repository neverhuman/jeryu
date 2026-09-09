//! File discovery and path classification for the tool-build scanner.
//!
//! Two discovery strategies:
//! * v1: the original recursive walker with a fixed deny-list. Byte-compatible
//!   with the original scanner; used by the compat wrappers.
//! * v2: `git ls-files` (gitignore-aware — the only thing that keeps repos with
//!   huge ignored datasets/venvs scannable in milliseconds) with a hardened
//!   walker fallback, plus path classification: generated zones, lockfiles,
//!   managed scaffold, test paths, and config files.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use super::ToolBuildScanOptions;
use crate::error::{CodeGraphError, Result};

/// How a repo-relative path participates in the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathClass {
    /// Regular product/application source.
    Source,
    /// Lives under a test path; occurrences are flagged.
    Test,
    /// Propagated on purpose by the family scaffold; clusters dominated by
    /// these become `managed-scaffold`, not tool candidates.
    ManagedScaffold,
    /// toml/yaml/json configuration.
    Config,
    /// Excluded from the scan entirely.
    Skip,
}

/// One discovered file ready for scanning.
#[derive(Debug, Clone)]
pub(crate) struct ClassifiedFile {
    pub abs: PathBuf,
    pub rel: String,
    pub language: &'static str,
    pub class: PathClass,
}

/// Discover one repo's scannable files under the v2 rules, returning the
/// files plus a count of files skipped by the corpus-scale top-dir guard.
/// Tolerant: an unreadable directory or a failed `git ls-files` falls
/// back/skips rather than aborting the multi-repo scan.
pub(crate) fn collect_repo_files(
    root: &Path,
    options: &ToolBuildScanOptions,
) -> (Vec<ClassifiedFile>, usize) {
    let zones = if options.honor_generated_zones {
        load_generated_zones(root)
    } else {
        Vec::new()
    };
    let mut rels: Vec<String> = Vec::new();
    let mut listed = false;
    if options.use_git_ls_files
        && root.join(".git").exists()
        && let Some(files) = git_ls_files(root)
    {
        rels = files;
        listed = true;
    }
    if !listed {
        let mut paths = Vec::new();
        collect_walker(root, &mut paths, true);
        rels = paths
            .into_iter()
            .map(|path| repo_relative(root, &path))
            .collect();
    }
    rels.sort();
    rels.dedup();

    let mut out = Vec::with_capacity(rels.len());
    for rel in rels {
        let abs = root.join(&rel);
        let Some(language) = language_for_rel(&rel) else {
            continue;
        };
        let class = classify_path(&rel, language, &zones);
        if class == PathClass::Skip {
            continue;
        }
        if !abs.is_file() {
            // git ls-files can list tracked-but-deleted paths.
            continue;
        }
        out.push(ClassifiedFile {
            abs,
            rel,
            language,
            class,
        });
    }

    // Corpus-scale guard: a top-level directory holding thousands of
    // scannable files is a dataset/corpus, not product code. Scanning it
    // would serialize the run AND flood clusters with non-product noise.
    let mut dropped = 0usize;
    if options.max_files_per_top_dir > 0 {
        let mut per_top: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for file in &out {
            *per_top.entry(top_segment(&file.rel)).or_default() += 1;
        }
        let oversized: std::collections::BTreeSet<String> = per_top
            .iter()
            .filter(|&(_, &count)| count > options.max_files_per_top_dir)
            .map(|(top, _)| (*top).to_string())
            .collect();
        if !oversized.is_empty() {
            let before = out.len();
            out.retain(|file| !oversized.contains(top_segment(&file.rel)));
            dropped = before - out.len();
        }
    }
    (out, dropped)
}

fn top_segment(rel: &str) -> &str {
    rel.split('/').next().unwrap_or(rel)
}

/// `git ls-files -z --cached --others --exclude-standard`: tracked plus
/// untracked-but-not-ignored, exactly the working-tree content an operator
/// thinks of as "the repo".
fn git_ls_files(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Some(
        stdout
            .split('\0')
            .filter(|rel| !rel.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// v1 walker: exact original behavior including strict IO error propagation
/// and the original deny-list. The compat wrappers depend on this byte-level
/// parity.
pub(crate) fn collect_source_files_v1(
    root: &Path,
    current: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(current).map_err(|source| CodeGraphError::Index {
        path: current.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CodeGraphError::Index {
            path: current.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CodeGraphError::Index {
            path: path.display().to_string(),
            source,
        })?;
        if file_type.is_dir() {
            if should_skip_dir_v1(root, &path) {
                continue;
            }
            collect_source_files_v1(root, &path, out)?;
        } else if file_type.is_file() && is_source_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// v2 fallback walker for non-git roots: tolerant of unreadable directories
/// and hardened against junk trees (any dot-directory except `.github`,
/// caches, vendored deps).
fn collect_walker(current: &Path, out: &mut Vec<PathBuf>, tolerant: bool) {
    let Ok(entries) = std::fs::read_dir(current) else {
        debug_assert!(tolerant, "v2 walker is always tolerant");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if should_skip_dir_v2(&path) {
                continue;
            }
            collect_walker(&path, out, tolerant);
        } else if file_type.is_file() && is_source_file(&path) {
            out.push(path);
        }
    }
}

fn should_skip_dir_v1(root: &Path, path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".jankurai"
            | "dist"
            | "playwright-report"
            | "storybook-static"
    ) || repo_relative(root, path).starts_with("docs/generated/")
}

fn should_skip_dir_v2(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    // Any dot-directory except .github: kills .git, .venv-*, .jankurai,
    // .build_*, .artifacts, .claude, .jeryu without enumerating them.
    if name.starts_with('.') && name != ".github" {
        return true;
    }
    matches!(
        name,
        "target"
            | "node_modules"
            | "dist"
            | "build"
            | "vendor"
            | "third_party"
            | "site-packages"
            | "__pycache__"
            | "playwright-report"
            | "storybook-static"
    )
}

pub(crate) fn is_source_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    language_for_ext(ext).is_some()
}

pub(crate) fn language_for_path(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(language_for_ext)
        .unwrap_or("unknown")
        .to_string()
}

fn language_for_rel(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit('.').next()?;
    language_for_ext(ext)
}

fn language_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescript_react",
        "js" => "javascript",
        "jsx" => "javascript_react",
        "go" => "go",
        "py" => "python",
        "java" => "java",
        "kt" => "kotlin",
        "rb" => "ruby",
        "sh" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        _ => return None,
    })
}

/// Whether the language counts as configuration for category purposes.
pub(crate) fn is_config_language(language: &str) -> bool {
    matches!(language, "toml" | "yaml" | "json")
}

pub(crate) fn repo_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Classify a repo-relative path under the v2 rules.
fn classify_path(rel: &str, language: &'static str, zones: &[String]) -> PathClass {
    let file_name = rel.rsplit('/').next().unwrap_or(rel);

    // Hard skips: lockfiles, minified bundles, generated trees, fixtures.
    if matches!(
        file_name,
        "Cargo.lock"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "poetry.lock"
            | "uv.lock"
            | "composer.lock"
            | "Gemfile.lock"
            | "flake.lock"
    ) {
        return PathClass::Skip;
    }
    if file_name.ends_with(".min.js")
        || file_name.ends_with(".min.css")
        || file_name.ends_with(".map")
    {
        return PathClass::Skip;
    }
    if rel.starts_with("docs/generated/")
        || rel.starts_with("contracts/generated/")
        || rel.starts_with("schemas/generated/")
        || rel.starts_with("generated/")
        || rel.contains("/generated/")
        || rel.starts_with("fixtures/")
        || rel.contains("/fixtures/")
        || rel.starts_with("dossiers/")
    {
        return PathClass::Skip;
    }
    for zone in zones {
        if glob_match(zone, rel) {
            return PathClass::Skip;
        }
    }

    if is_managed_scaffold(rel, file_name) {
        return PathClass::ManagedScaffold;
    }
    if is_test_path(rel, file_name) {
        return PathClass::Test;
    }
    if is_config_language(language) {
        return PathClass::Config;
    }
    PathClass::Source
}

/// Paths the family scaffold renderer propagates across repos on purpose.
/// Duplication here is governed by `jeryu-tool`, not a tool-build lead.
fn is_managed_scaffold(rel: &str, file_name: &str) -> bool {
    if matches!(
        file_name,
        "Justfile"
            | "justfile"
            | "rust-toolchain.toml"
            | "deny.toml"
            | "gitleaks.toml"
            | ".gitignore"
            | "ci-fast-push.sh"
            | "pr-ci.sh"
    ) {
        return true;
    }
    rel.starts_with("ops/ci/")
        || rel.starts_with("ops/git-hooks/")
        || (rel.starts_with("ops/") && rel.ends_with(".sh") && !rel[4..].contains('/'))
        || rel.starts_with(".github/workflows/")
        || rel.starts_with("agent/")
        || (rel.starts_with("scripts/") && file_name.starts_with("ci-"))
}

fn is_test_path(rel: &str, file_name: &str) -> bool {
    if rel.starts_with("tests/")
        || rel.contains("/tests/")
        || rel.contains("/test/")
        || rel.starts_with("test/")
        || rel.contains("/__tests__/")
    {
        return true;
    }
    file_name.ends_with("_test.rs")
        || file_name.ends_with("_tests.rs")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || file_name.starts_with("test_") && file_name.ends_with(".py")
        || file_name == "conftest.py"
}

#[derive(Debug, Deserialize)]
struct GeneratedZonesFile {
    #[serde(default)]
    zones: Vec<GeneratedZone>,
}

#[derive(Debug, Deserialize)]
struct GeneratedZone {
    path: Option<String>,
}

/// Load a repo's `agent/generated-zones.toml` skip globs (family standard).
/// Missing or malformed files yield no zones — discovery must never fail on
/// metadata drift.
fn load_generated_zones(root: &Path) -> Vec<String> {
    let path = root.join("agent").join("generated-zones.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(parsed) = toml::from_str::<GeneratedZonesFile>(&text) else {
        return Vec::new();
    };
    parsed
        .zones
        .into_iter()
        .filter_map(|zone| zone.path)
        .collect()
}

/// Minimal glob matcher over `/`-separated paths supporting `**` (any number
/// of segments), `*` (within one segment), and literal segments. Covers the
/// `".jankurai/**"` / `"generated/**"` / `"*.lock"` shapes generated-zones
/// files use without pulling in a glob crate.
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    match_segments(&pattern_segments, &path_segments)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => {
            // `**` matches zero or more whole segments.
            (0..=path.len()).any(|skip| match_segments(rest, &path[skip..]))
        }
        Some((first, rest)) => match path.split_first() {
            None => false,
            Some((segment, path_rest)) => {
                match_segment(first, segment) && match_segments(rest, path_rest)
            }
        },
    }
}

fn match_segment(pattern: &str, segment: &str) -> bool {
    // Within-segment `*` wildcard via greedy backtracking over byte indices.
    fn matches(pattern: &[u8], segment: &[u8]) -> bool {
        match pattern.split_first() {
            None => segment.is_empty(),
            Some((b'*', rest)) => (0..=segment.len()).any(|skip| matches(rest, &segment[skip..])),
            Some((ch, rest)) => segment
                .split_first()
                .is_some_and(|(s, seg_rest)| s == ch && matches(rest, seg_rest)),
        }
    }
    matches(pattern.as_bytes(), segment.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matcher_covers_zone_shapes() {
        assert!(glob_match(".jankurai/**", ".jankurai/report.json"));
        assert!(glob_match(".jankurai/**", ".jankurai/deep/nested/file.rs"));
        assert!(glob_match("generated/**", "generated/types.ts"));
        assert!(!glob_match("generated/**", "src/generated.rs"));
        assert!(glob_match("*.lock", "Cargo.lock"));
        assert!(glob_match("src/**/*.rs", "src/a/b/lib.rs"));
        assert!(glob_match("src/**/*.rs", "src/lib.rs"));
        assert!(!glob_match("src/*.rs", "src/a/lib.rs"));
    }

    #[test]
    fn classify_skips_lockfiles_minified_and_generated() {
        assert_eq!(classify_path("Cargo.lock", "toml", &[]), PathClass::Skip);
        assert_eq!(
            classify_path("dist/app.min.js", "javascript", &[]),
            PathClass::Skip
        );
        assert_eq!(
            classify_path("contracts/generated/Foo.ts", "typescript", &[]),
            PathClass::Skip
        );
        assert_eq!(
            classify_path("fixtures/sample.json", "json", &[]),
            PathClass::Skip
        );
        assert_eq!(
            classify_path(
                ".jankurai/report.json",
                "json",
                &[".jankurai/**".to_string()]
            ),
            PathClass::Skip
        );
    }

    #[test]
    fn classify_labels_scaffold_test_config_source() {
        assert_eq!(
            classify_path("ops/ci/check.sh", "shell", &[]),
            PathClass::ManagedScaffold
        );
        assert_eq!(
            classify_path(".github/workflows/ci.yml", "yaml", &[]),
            PathClass::ManagedScaffold
        );
        assert_eq!(
            classify_path("agent/boundaries.toml", "toml", &[]),
            PathClass::ManagedScaffold
        );
        assert_eq!(
            classify_path("Justfile", "unknown", &[]),
            PathClass::ManagedScaffold
        );
        assert_eq!(
            classify_path("crates/x/tests/integration.rs", "rust", &[]),
            PathClass::Test
        );
        assert_eq!(
            classify_path("src/pages/__tests__/a.test.tsx", "typescript_react", &[]),
            PathClass::Test
        );
        assert_eq!(
            classify_path("config/app.yaml", "yaml", &[]),
            PathClass::Config
        );
        assert_eq!(classify_path("src/lib.rs", "rust", &[]), PathClass::Source);
    }

    #[test]
    fn v2_walker_skips_dot_dirs_and_junk() {
        assert!(should_skip_dir_v2(Path::new("/r/.venv-py311")));
        assert!(should_skip_dir_v2(Path::new("/r/__pycache__")));
        assert!(should_skip_dir_v2(Path::new("/r/node_modules")));
        assert!(!should_skip_dir_v2(Path::new("/r/.github")));
        assert!(!should_skip_dir_v2(Path::new("/r/src")));
    }
}
