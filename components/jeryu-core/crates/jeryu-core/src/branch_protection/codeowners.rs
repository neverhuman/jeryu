//! Minimal CODEOWNERS parsing and path-glob matching.

/// Minimal CODEOWNERS model: ordered `(glob, owners)` rules. As on GitHub the
/// *last* matching rule for a path wins.
#[derive(Debug, Default)]
pub(super) struct CodeOwners {
    rules: Vec<(String, Vec<String>)>,
}

impl CodeOwners {
    pub(super) fn parse(contents: &str) -> Self {
        let mut rules = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let owners: Vec<String> = parts
                .map(|owner| owner.trim_start_matches('@').to_string())
                .filter(|owner| !owner.is_empty())
                .collect();
            rules.push((pattern.to_string(), owners));
        }
        Self { rules }
    }

    /// Owners for a path: the owners of the last matching rule (GitHub order).
    /// A path with no matching rule is unowned — an explicit empty owner set,
    /// not a swallowed error.
    pub(super) fn owners_for(&self, path: &str) -> Vec<String> {
        match self
            .rules
            .iter()
            .rev()
            .find(|(pattern, _)| glob_match(pattern, path))
        {
            Some((_, owners)) => owners.clone(),
            None => Vec::new(),
        }
    }
}

/// Minimal CODEOWNERS path-glob match. Supports a leading `*` extension glob
/// (e.g. `*.rs`), directory prefixes (`docs/`, `/src/`), and a `/**` suffix.
fn glob_match(pattern: &str, path: &str) -> bool {
    let path = path.trim_start_matches('/');

    // `*` matches everything.
    if pattern == "*" {
        return true;
    }

    // Extension / filename glob: `*.rs`, `*.md`.
    if let Some(suffix) = pattern.strip_prefix('*') {
        return path.ends_with(suffix);
    }

    let pat = pattern.trim_start_matches('/');

    // Recursive directory glob: `src/**` matches anything under `src/`.
    if let Some(prefix) = pat.strip_suffix("/**") {
        let prefix = prefix.trim_end_matches('/');
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }

    // Directory prefix: `docs/` matches everything under `docs/`.
    if let Some(prefix) = pat.strip_suffix('/') {
        return path.starts_with(&format!("{prefix}/")) || path == prefix;
    }

    // A bare directory name (no trailing slash) also covers its contents, the
    // way GitHub treats `docs` as `docs/`.
    if path == pat || path.starts_with(&format!("{pat}/")) {
        return true;
    }

    false
}
