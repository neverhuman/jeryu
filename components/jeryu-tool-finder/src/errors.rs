//! Agent-friendly typed exception pattern: every operator-facing failure
//! carries its purpose, the concrete reason, common fixes, a docs pointer,
//! and a repair_hint, so the next rerun is local instead of a support thread.
//! Mirrors the family's typed-error envelope (jeryu-api `TypedError`,
//! codegraph misses).

use std::fmt;

/// A typed, repairable failure.
#[derive(Debug)]
pub struct FinderError {
    /// What the finder was trying to do.
    pub purpose: &'static str,
    /// Why it failed, concretely.
    pub reason: String,
    /// The fixes that usually resolve this.
    pub common_fixes: &'static [&'static str],
    /// Where the durable documentation lives.
    pub docs_url: &'static str,
    /// The next command/action that makes the rerun succeed.
    pub repair_hint: &'static str,
}

impl FinderError {
    pub fn new(
        purpose: &'static str,
        reason: impl Into<String>,
        common_fixes: &'static [&'static str],
        repair_hint: &'static str,
    ) -> Self {
        Self {
            purpose,
            reason: reason.into(),
            common_fixes,
            docs_url: "docs/tool-finder.md",
            repair_hint,
        }
    }
}

impl fmt::Display for FinderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}: {}", self.purpose, self.reason)?;
        for fix in self.common_fixes {
            writeln!(f, "  fix: {fix}")?;
        }
        writeln!(f, "  docs: {}", self.docs_url)?;
        write!(f, "  repair: {}", self.repair_hint)
    }
}

impl std::error::Error for FinderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_errors_carry_the_repair_surface() {
        let error = FinderError::new(
            "scan the split families",
            "no manifests found",
            &["run from the finder repo root inside the split checkout"],
            "cd into the split checkout, then rerun `jeryu-tool-finder scan`",
        );
        let rendered = error.to_string();
        assert!(rendered.contains("scan the split families"));
        assert!(rendered.contains("fix:"));
        assert!(rendered.contains("docs/tool-finder.md"));
        assert!(rendered.contains("repair:"));
    }
}
