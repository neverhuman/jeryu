//! Minimal path matcher supporting exact, prefix, and `/**` rules.

/// Path pattern matcher for owner/test/generated maps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathPattern {
    pattern: String,
}

impl PathPattern {
    /// Creates a path pattern.
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
        }
    }

    /// Returns the pattern string.
    pub fn as_str(&self) -> &str {
        &self.pattern
    }

    /// Checks whether a path matches the pattern.
    pub fn matches(&self, path: &str) -> bool {
        if self.pattern == "**" || self.pattern == "*" {
            return true;
        }
        if let Some(prefix) = self.pattern.strip_suffix("/**") {
            return path == prefix || path.starts_with(&format!("{prefix}/"));
        }
        if let Some(prefix) = self.pattern.strip_suffix('*') {
            return path.starts_with(prefix);
        }
        self.pattern == path
    }
}

#[cfg(test)]
mod tests {
    use super::PathPattern;

    #[test]
    fn double_star_matches_prefix_tree() {
        let p = PathPattern::new("crates/jeryu_proof/**");
        assert!(p.matches("crates/jeryu_proof/src/lib.rs"));
        assert!(p.matches("crates/jeryu_proof"));
        assert!(!p.matches("crates/jeryu_agentbridge/src/lib.rs"));
    }
}
