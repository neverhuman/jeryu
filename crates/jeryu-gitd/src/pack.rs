//! Pack protocol adapters.

use crate::command::{run_capture, run_with_stdin};
use crate::error::Result;
use crate::repo::Repository;

/// Git pack service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackService {
    /// `git-upload-pack`.
    UploadPack,
    /// `git-receive-pack`.
    ReceivePack,
}

impl PackService {
    /// HTTP service name.
    #[must_use]
    pub fn http_name(self) -> &'static str {
        match self {
            Self::UploadPack => "git-upload-pack",
            Self::ReceivePack => "git-receive-pack",
        }
    }

    /// Git subcommand.
    #[must_use]
    pub fn git_subcommand(self) -> &'static str {
        match self {
            Self::UploadPack => "upload-pack",
            Self::ReceivePack => "receive-pack",
        }
    }

    /// Parse a service name.
    #[must_use]
    pub fn parse(service: &str) -> Option<Self> {
        match service {
            "git-upload-pack" => Some(Self::UploadPack),
            "git-receive-pack" => Some(Self::ReceivePack),
            _ => None,
        }
    }
}

/// Advertise refs for smart HTTP.
pub fn advertise_refs(git_bin: &str, repo: &Repository, service: PackService) -> Result<Vec<u8>> {
    let path = repo.path.to_string_lossy().to_string();
    let command = service.git_subcommand();
    let out = run_capture(
        git_bin,
        &[command, "--stateless-rpc", "--advertise-refs", &path],
        None,
    )?;
    Ok(out.stdout)
}

/// Execute a stateless RPC exchange.
pub fn stateless_rpc(
    git_bin: &str,
    repo: &Repository,
    service: PackService,
    body: &[u8],
) -> Result<Vec<u8>> {
    let path = repo.path.to_string_lossy().to_string();
    let command = service.git_subcommand();
    let out = run_with_stdin(git_bin, &[command, "--stateless-rpc", &path], body, None)?;
    Ok(out.stdout)
}
