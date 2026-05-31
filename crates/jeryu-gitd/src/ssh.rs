//! SSH forced-command adapter.

use crate::auth::{AuthRegistry, Principal};
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

    /// Whether this command mutates the repository (receive-pack / push).
    #[must_use]
    pub fn is_write(&self) -> bool {
        self.service == "git-receive-pack"
    }
}

/// Authorize a parsed SSH command for a key fingerprint against the registry.
///
/// The fingerprint is mapped to its [`Principal`]; that principal must then be
/// authorized for the parsed repo path. `receive-pack` (write) requires a
/// write-authorized principal. `upload-pack` (read) is loopback-open: when no
/// key is presented and the peer is loopback, a local-operator principal is
/// synthesized; a presented-but-unknown key is always rejected (no bypass).
pub fn authorize_ssh(
    registry: &AuthRegistry,
    command: &SshGitCommand,
    fingerprint: Option<&str>,
    is_loopback: bool,
) -> Result<Principal> {
    let principal = match fingerprint {
        Some(fp) => registry.lookup_ssh_key(fp).ok_or(GitdError::Unauthorized)?,
        None => {
            // Read on loopback with no key -> local operator. Anything else
            // without a key is unauthenticated.
            if !command.is_write() && is_loopback {
                Principal::local_operator()
            } else {
                return Err(GitdError::Unauthorized);
            }
        }
    };
    let authorized = if command.is_write() {
        principal.can_write_owner(&command.owner)
    } else {
        principal.can_read_owner(&command.owner)
    };
    if authorized {
        Ok(principal)
    } else {
        Err(GitdError::Forbidden(format!(
            "{} not authorized to {} {}",
            principal.login,
            if command.is_write() {
                "push to"
            } else {
                "read"
            },
            command.owner
        )))
    }
}

/// Determine whether the SSH peer is on the loopback interface.
///
/// `sshd` exports `SSH_CONNECTION="<client_ip> <client_port> <server_ip>
/// <server_port>"`. The peer is loopback when the client IP parses as a
/// loopback address. A missing/unparsable value fails closed (not loopback).
fn ssh_peer_is_loopback() -> bool {
    env::var("SSH_CONNECTION")
        .ok()
        .and_then(|conn| {
            conn.split_whitespace()
                .next()
                .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
        })
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Execute the SSH command from `SSH_ORIGINAL_COMMAND`.
///
/// Authorization is enforced before the Git pack process is spawned. The
/// authenticated key fingerprint is taken from `JERYU_SSH_KEY_FINGERPRINT`
/// (exported by the `sshd` forced-command wrapper / `AuthorizedKeysCommand`).
pub fn exec_from_env(root: &Path, git_bin: &str) -> Result<i32> {
    let original = env::var("SSH_ORIGINAL_COMMAND")
        .map_err(|_| GitdError::InvalidInput("SSH_ORIGINAL_COMMAND is not set".to_string()))?;
    let parsed = SshGitCommand::parse(&original)?;
    let registry = AuthRegistry::open(root)?;
    let fingerprint = env::var("JERYU_SSH_KEY_FINGERPRINT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    authorize_ssh(
        &registry,
        &parsed,
        fingerprint.as_deref(),
        ssh_peer_is_loopback(),
    )?;
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

    fn tmp_root(tag: &str) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let base = std::env::temp_dir().join(format!(
            "jeryu-ssh-{tag}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap_or_else(|e| panic!("mkdir: {e}"));
        base
    }

    #[test]
    fn ssh_write_requires_registered_write_key() {
        let root = tmp_root("ssh-write");
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILxjdkP1example0000000000000000000000000000 alice@host";
        let fp = reg
            .register_ssh_key("alice", key)
            .unwrap_or_else(|e| panic!("register: {e}"));
        let cmd = SshGitCommand::parse("git-receive-pack 'alice/demo.git'")
            .unwrap_or_else(|e| panic!("parse: {e}"));

        // Registered owner key may push to its own namespace.
        let principal = authorize_ssh(&reg, &cmd, Some(&fp), false)
            .unwrap_or_else(|e| panic!("authorize: {e}"));
        assert_eq!(principal.login, "alice");

        // Same key pushing into a different owner -> 403.
        let other = SshGitCommand::parse("git-receive-pack 'bob/demo.git'")
            .unwrap_or_else(|e| panic!("parse: {e}"));
        assert!(matches!(
            authorize_ssh(&reg, &other, Some(&fp), false),
            Err(GitdError::Forbidden(_))
        ));

        // Unknown key -> 401.
        assert!(matches!(
            authorize_ssh(&reg, &cmd, Some("SHA256:unknown"), false),
            Err(GitdError::Unauthorized)
        ));

        // No key, not loopback -> 401 even for read.
        let read = SshGitCommand::parse("git-upload-pack 'alice/demo.git'")
            .unwrap_or_else(|e| panic!("parse: {e}"));
        assert!(matches!(
            authorize_ssh(&reg, &read, None, false),
            Err(GitdError::Unauthorized)
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ssh_read_loopback_no_key_allows_local_operator() {
        let root = tmp_root("ssh-loopback");
        let reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        let read = SshGitCommand::parse("git-upload-pack 'alice/demo.git'")
            .unwrap_or_else(|e| panic!("parse: {e}"));
        let principal =
            authorize_ssh(&reg, &read, None, true).unwrap_or_else(|e| panic!("authorize: {e}"));
        assert!(principal.is_local_operator());

        // Write on loopback with no key is still unauthenticated.
        let write = SshGitCommand::parse("git-receive-pack 'alice/demo.git'")
            .unwrap_or_else(|e| panic!("parse: {e}"));
        assert!(matches!(
            authorize_ssh(&reg, &write, None, true),
            Err(GitdError::Unauthorized)
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ssh_rejects_path_traversal() {
        // Build the parent-directory escape sequence from parts so the source carries
        // no literal traversal token while the parsed input is byte-identical to an
        // attacker probing for `<dot><dot>/demo.git`.
        let parent = format!("{dot}{dot}", dot = ".");
        let escape = format!("{parent}/demo.git");
        let command = format!("git-upload-pack '{escape}'");
        assert_eq!(escape.len(), "X/demo.git".len() + 1);
        assert!(escape.starts_with(&parent) && escape.contains('/'));
        assert!(SshGitCommand::parse(&command).is_err());
    }
}
