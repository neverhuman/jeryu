use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

use crate::git::system::SystemGit;
use crate::redact::redact_text;

use super::{LocalRepoConfig, local_gitlab_ssh_url, mirror_path};

pub(crate) fn refresh_bare_mirror(config: &LocalRepoConfig) -> Result<PathBuf> {
    let mirror = mirror_path(config);
    let local_url = local_gitlab_ssh_url(&config.repo);
    if mirror.is_dir() {
        run_git_checked(&mirror, &["remote", "set-url", "origin", &local_url])?;
        run_git_checked(
            &mirror,
            &[
                "fetch",
                "--prune",
                "origin",
                "+refs/heads/*:refs/heads/*",
                "+refs/tags/*:refs/tags/*",
            ],
        )
        .with_context(|| format!("fetching {}", redact_text(&local_url)))?;
    } else {
        let parent = mirror
            .parent()
            .ok_or_else(|| anyhow::anyhow!("mirror path has no parent: {}", mirror.display()))?;
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        run_git_checked(
            parent,
            &[
                "clone",
                "--mirror",
                &local_url,
                mirror.to_string_lossy().as_ref(),
            ],
        )
        .with_context(|| format!("cloning local GitLab mirror for {}", config.repo))?;
    }
    Ok(mirror)
}

pub(crate) fn run_git_checked(cwd: &Path, args: &[&str]) -> Result<Output> {
    let git = SystemGit::resolve()?;
    run_checked(Command::new(git.path).current_dir(cwd).args(args))
        .with_context(|| format!("running git {:?}", args))
}

pub(crate) fn run_checked(command: &mut Command) -> Result<Output> {
    let output = command.output().context("spawning command")?;
    if output.status.success() {
        return Ok(output);
    }
    bail!(
        "command failed: {}",
        redact_text(&String::from_utf8_lossy(&output.stderr))
    )
}
