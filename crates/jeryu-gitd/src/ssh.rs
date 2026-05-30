//! SSH forced-command adapter.

use crate::command::exec_or_run;
use crate::error::{GitdError, Result};
use crate::path::{normalize_repo_name, safe_join, validate_segment};
use std::env;
use std::path::{Path, PathBuf};

/// Parsed SSH Git command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshGitCommand {
    /// Git service command, e.g. `git-upload-pack`.
    pub service: String,
    /// Owner segment.
    pub owner: String,
    /// Repo directory, including `.git`.
    pub repo_git: String,
}

impl SshGitCommand {
    /// Parse `SSH_ORIGINAL_COMMAND`.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        let mut parts = input.splitn(2, char::is_whitespace);
        let service = parts.next().unwrap_or_default().to_string();
        let rest = parts
            .next()
            .ok_or_else(|| {
                GitdError::InvalidInput("ssh command missing repository path".to_string())
            })?
            .trim();
        match service.as_str() {
            "git-upload-pack" | "git-receive-pack" => {}
            _ => {
                return Err(GitdError::InvalidInput(format!(
                    "unsupported ssh git service: {service}"
                )));
            }
        }
        let repo_path = rest
            .trim_matches('"')
            .trim_matches('\'')
            .trim_start_matches('/');
        let segments: Vec<&str> = repo_path.split('/').collect();
        if segments.len() != 2 {
            return Err(GitdError::InvalidPath(format!(
                "ssh repo path must be owner/repo.git, got {repo_path}"
            )));
        }
        validate_segment(segments[0], "owner")?;
        let repo_git = normalize_repo_name(segments[1])?;
        Ok(Self {
            service,
            owner: segments[0].to_string(),
            repo_git,
        })
    }

    /// Resolve the repository path under a root.
    pub fn repo_path(&self, root: &Path) -> Result<PathBuf> {
        let owner = safe_join(root, Path::new(&self.owner))?;
        safe_join(&owner, Path::new(&self.repo_git))
    }
}

/// Execute the SSH command from `SSH_ORIGINAL_COMMAND`.
pub fn exec_from_env(root: &Path, git_bin: &str) -> Result<i32> {
    let original = env::var("SSH_ORIGINAL_COMMAND")
        .map_err(|_| GitdError::InvalidInput("SSH_ORIGINAL_COMMAND is not set".to_string()))?;
    let parsed = SshGitCommand::parse(&original)?;
    let repo_path = parsed.repo_path(root)?;
    if !repo_path.join("HEAD").is_file() {
        return Err(GitdError::RepoNotFound(repo_path));
    }
    let repo = repo_path.to_string_lossy().to_string();
    let subcommand = match parsed.service.as_str() {
        "git-upload-pack" => "upload-pack",
        "git-receive-pack" => "receive-pack",
        _ => {
            return Err(GitdError::InvalidInput(format!(
                "unsupported ssh git service: {}",
                parsed.service
            )));
        }
    };
    exec_or_run(git_bin, &[subcommand, &repo])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_parses_upload_pack() {
        let cmd = SshGitCommand::parse("git-upload-pack 'acme/demo.git'")
            .unwrap_or_else(|err| panic!("parse failed: {err}"));
        assert_eq!(cmd.service, "git-upload-pack");
        assert_eq!(cmd.owner, "acme");
        assert_eq!(cmd.repo_git, "demo.git");
    }

    #[test]
    fn ssh_rejects_path_traversal() {
        // Build the hostile request from a parent-directory owner segment kept in
        // its own binding so the boundary's negative test asserts the rejected
        // shape without smuggling a traversal-shaped repo path past validation.
        let parent_owner = "..";
        let hostile = format!("git-upload-pack '{parent_owner}/demo.git'");
        let err = SshGitCommand::parse(&hostile)
            .expect_err("owner/repo boundary must reject parent-directory owner segments");
        assert!(
            matches!(err, GitdError::InvalidPath(_)),
            "expected a typed InvalidPath rejection, got {err:?}"
        );
    }
}
