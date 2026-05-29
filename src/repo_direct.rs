use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use super::{HookMode, HookProfile, RepoMode};

#[path = "repo_direct_gitlab.rs"]
mod repo_direct_gitlab;
pub(crate) use repo_direct_gitlab::{
    ensure_local_gitlab_project, local_gitlab_reachable, local_gitlab_ssh_url,
    seed_missing_local_branch,
};

#[path = "repo_direct_hooks.rs"]
mod repo_direct_hooks;
pub(crate) use repo_direct_hooks::{configure_hook_mode, hook_label, mode_label};

#[derive(Clone, Copy, Debug)]
pub(super) struct JeryuConfigSpec<'a> {
    pub(super) mode: RepoMode,
    pub(super) hooks: HookMode,
    pub(super) namespace: &'a str,
    pub(super) name: &'a str,
    pub(super) branch: &'a str,
    pub(super) protect_main: bool,
    pub(super) main_relay: bool,
    pub(super) offline_release_remote: Option<&'a str>,
}

pub(super) fn configure_remote(
    repo_root: &Path,
    remote_name: &str,
    remote_url: &str,
    replace_origin: bool,
) -> Result<()> {
    if remote_name == "origin" && replace_origin && git_remote_exists(repo_root, "origin") {
        run_git(repo_root, &["remote", "set-url", "origin", remote_url])?;
        return Ok(());
    }
    if git_remote_exists(repo_root, remote_name) {
        run_git(repo_root, &["remote", "set-url", remote_name, remote_url])?;
    } else {
        run_git(repo_root, &["remote", "add", remote_name, remote_url])?;
    }
    Ok(())
}

pub(super) fn remove_other_remotes(repo_root: &Path, keep: &str) -> Result<()> {
    let output = git_output(repo_root, &["remote"]).context("listing git remotes")?;
    if !output.status.success() {
        bail!(
            "git remote failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    for remote in String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty() && *remote != keep)
    {
        run_git(repo_root, &["remote", "remove", remote])?;
    }
    Ok(())
}

fn git_remote_exists(repo_root: &Path, name: &str) -> bool {
    git_output(repo_root, &["remote", "get-url", name])
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(super) fn write_jeryu_configs(repo_root: &Path, spec: JeryuConfigSpec<'_>) -> Result<()> {
    let dir = repo_root.join(".jeryu");
    fs::create_dir_all(&dir)?;
    write_file_if_changed(
        &dir.join("repo.toml"),
        &format!(
            "schema_version = \"1\"\nmode = \"{}\"\nnamespace = \"{}\"\nname = \"{}\"\ndefault_branch = \"{}\"\nremote_url = \"{}\"\n",
            mode_label(spec.mode),
            spec.namespace,
            spec.name,
            spec.branch,
            local_gitlab_ssh_url(spec.namespace, spec.name)
        ),
    )?;
    write_file_if_changed(
        &dir.join("policy.toml"),
        &format!(
            "schema_version = \"1\"\nprotect_main = {}\nprotected_branches = [\"{}\"]\nprotected_tags = [\"v*\"]\nhooks = \"{}\"\ndirect_push_to_main = \"deny\"\nmerge_request_required = true\nlinear_history_required = true\nbranch_must_be_rebased_on_base = true\n\n[main_relay]\nenabled = {}\nactor = \"jeryu\"\nprotected_branch = \"{}\"\nrequire_admission_receipt = true\n\n[offline_release_mirror]\nenabled = {}\nremote = \"{}\"\nrefs = [\"refs/tags/v*\", \"refs/heads/release/*\"]\n",
            spec.protect_main,
            spec.branch,
            hook_label(spec.hooks),
            spec.main_relay,
            spec.branch,
            spec.offline_release_remote.is_some(),
            spec.offline_release_remote.unwrap_or("")
        ),
    )?;
    write_file_if_changed(
        &dir.join("backup.toml"),
        "schema_version = \"1\"\nlocal_bundle = true\nbare_mirror = true\nprune_mirror = false\nssh_target = \"\"\nrequire_fresh_backup = \"warn\"\n",
    )?;
    write_file_if_changed(
        &dir.join("ci.toml"),
        "schema_version = \"1\"\ngithub_actions_required = false\nlocal_gitlab_required = true\n",
    )?;
    Ok(())
}

fn write_file_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

pub(super) fn run_git(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = git_output(repo_root, args).with_context(|| format!("running git {:?}", args))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub(super) fn git_config_get(repo_root: &Path, key: &str) -> Result<Option<String>> {
    let output = git_output(repo_root, &["config", "--local", "--get", key])
        .with_context(|| format!("reading git config {key}"))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

pub(super) fn unset_git_config(repo_root: &Path, key: &str) -> Result<()> {
    let _ = git_output(repo_root, &["config", "--local", "--unset-all", key]);
    Ok(())
}

pub(super) fn git_output(repo_root: &Path, args: &[&str]) -> std::io::Result<Output> {
    #[cfg(test)]
    let _guard = crate::test_sync::PATH_ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
}

pub(super) fn configure_git_hooks(repo_root: &Path) -> Result<()> {
    repo_direct_hooks::configure_git_hooks(repo_root)
}

#[cfg(all(test, unix))]
#[path = "repo_direct_tests.rs"]
mod tests;
