/// Check if a file path matches a simple glob pattern.
///
/// Supported patterns:
/// - `foo/bar` — exact match
/// - `foo/*` — matches any file under `foo/` (single level)
/// - `foo/**` — matches any file under `foo/` (recursive)
/// - `*.ext` — matches any file ending with `.ext`
/// - `Foo*` — matches any file starting with `Foo`
/// - `**/foo` — matches `foo` anywhere in the path
/// - `dir/**/*.ext` — matches `*.ext` recursively under `dir/`
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }

    // Pattern: `dir/*` → match anything directly under `dir/`
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path.starts_with(prefix)
            && path.len() > prefix.len() + 1
            && !path[prefix.len() + 1..].contains('/');
    }

    // Pattern: `dir/**` → match anything recursively under `dir/`
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path.starts_with(prefix) && path.len() > prefix.len() + 1;
    }

    // Pattern: `dir/**/*.ext` → match `*.ext` recursively under `dir/`
    if pattern.contains("/**/") {
        let parts: Vec<&str> = pattern.splitn(2, "/**/").collect();
        if parts.len() == 2 {
            let dir_prefix = parts[0];
            let tail = parts[1];
            if !path.starts_with(dir_prefix) || path.len() <= dir_prefix.len() + 1 {
                return false;
            }
            let remainder = &path[dir_prefix.len() + 1..];
            // Tail is typically `*.ext` or a literal
            return glob_match(tail, remainder)
                || remainder
                    .rsplit('/')
                    .next()
                    .map(|basename| glob_match(tail, basename))
                    .unwrap_or(false);
        }
    }

    // Pattern: `*.ext` — match any file ending with `.ext`
    // Pattern: `Foo*` — match any file starting with `Foo`
    if let Some(pos) = pattern.find('*') {
        let prefix = &pattern[..pos];
        let suffix = &pattern[pos + 1..];
        // For basename-level patterns (no `/` in pattern), match against the
        // full path or the basename.
        if !pattern.contains('/') {
            let basename = path.rsplit('/').next().unwrap_or(path);
            return basename.starts_with(prefix) && basename.ends_with(suffix);
        }
        return path.starts_with(prefix) && path.ends_with(suffix);
    }

    // Pattern: `**/foo` → match `foo` as the last component
    if let Some(suffix) = pattern.strip_prefix("**/") {
        return path == suffix || path.ends_with(&format!("/{}", suffix));
    }

    false
}

/// Check if a path matches any of a set of patterns.
pub fn matches_any(path: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| glob_match(p, path))
}

use super::{DOCS_PATTERNS, GLOBAL_INVALIDATORS, SUBSYSTEMS, Subsystem};

/// Find all subsystems affected by a set of changed paths.
pub fn affected_subsystems(changed_paths: &[String]) -> Vec<&'static Subsystem> {
    let mut affected = Vec::new();
    for subsystem in SUBSYSTEMS {
        let owns_changed = changed_paths
            .iter()
            .any(|p| matches_any(p, subsystem.owned_paths));
        if owns_changed {
            affected.push(subsystem);
        }
    }
    affected
}

/// Check if any changed path is a global invalidator.
pub fn has_global_invalidator(changed_paths: &[String]) -> Option<String> {
    for path in changed_paths {
        if matches_any(path, GLOBAL_INVALIDATORS) {
            return Some(path.clone());
        }
    }
    None
}

/// Check if all changed paths are documentation-only.
pub fn is_docs_only(changed_paths: &[String]) -> bool {
    !changed_paths.is_empty() && changed_paths.iter().all(|p| matches_any(p, DOCS_PATTERNS))
}

/// Check if any affected subsystem has force_full_paths that match the changes.
pub fn has_subsystem_force_full(
    changed_paths: &[String],
    affected: &[&Subsystem],
) -> Option<String> {
    for subsystem in affected {
        if subsystem.force_full_paths.is_empty() {
            continue;
        }
        for path in changed_paths {
            if matches_any(path, subsystem.force_full_paths) {
                return Some(format!(
                    "subsystem '{}' force-full trigger: {}",
                    subsystem.id, path
                ));
            }
        }
    }
    None
}
