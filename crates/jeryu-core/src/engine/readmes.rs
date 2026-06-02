//! Repository README persistence.

use super::ForgeCore;
use crate::errors::Result;

impl ForgeCore {
    /// Return the persisted README markdown for `owner/repo`, if one exists.
    pub fn get_repository_readme(&self, owner: &str, repo: &str) -> Result<Option<String>> {
        self.ensure_repo_exists(owner, repo)?;
        Ok(self
            .state
            .read()
            .readmes
            .get(&(owner.to_string(), repo.to_string()))
            .cloned())
    }

    /// Persist the canonical README markdown for `owner/repo` and return the
    /// stored source verbatim.
    pub fn set_repository_readme(
        &self,
        owner: &str,
        repo: &str,
        markdown: String,
    ) -> Result<String> {
        self.ensure_repo_exists(owner, repo)?;
        let mut state = self.state.write();
        let previous = state.clone();
        state
            .readmes
            .insert((owner.to_string(), repo.to_string()), markdown.clone());
        self.persist_after_mutation(&mut state, previous)?;
        Ok(markdown)
    }

    /// Return the persisted README or a deterministic local fallback.
    pub fn readme_or_default(&self, owner: &str, repo: &str, fallback: String) -> Result<String> {
        Ok(self.get_repository_readme(owner, repo)?.unwrap_or(fallback))
    }
}
