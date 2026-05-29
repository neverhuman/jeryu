use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use std::path::PathBuf;

#[path = "repo_direct.rs"]
mod repo_direct;
use repo_direct::{
    JeryuConfigSpec, configure_git_hooks, configure_hook_mode, git_config_get, unset_git_config,
    write_jeryu_configs,
};

#[path = "repo_capture.rs"]
mod repo_capture;
pub use repo_capture::capture_tui_screenshots;

#[path = "repo_ops.rs"]
mod repo_ops;
#[path = "repo_ops_setup.rs"]
mod repo_ops_setup;
pub(crate) use repo_ops::git_repo_root;
pub use repo_ops::{jankurai_fast, state_proof};
use repo_ops_setup::setup_direct_repo;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RepoMode {
    Direct,
    Observed,
    Enforced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum HookMode {
    Off,
    Advisory,
    Enforce,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum HookProfile {
    PrePush,
    PreCommitJankurai,
    All,
}

pub struct DirectRepoOptions {
    pub path: PathBuf,
    pub name: String,
    pub namespace: String,
    pub branch: String,
    pub protect_main: bool,
    pub hooks: HookMode,
    pub replace_origin: bool,
    pub new_repo: bool,
    pub dry_run: bool,
    pub main_relay: bool,
    pub offline_release_remote: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DirectRepoPlan {
    pub action: String,
    pub repo_path: String,
    pub namespace: String,
    pub name: String,
    pub branch: String,
    pub remote_name: String,
    pub remote_url: String,
    pub mode: RepoMode,
    pub hooks: HookMode,
    pub protect_main: bool,
    pub main_relay: bool,
    pub offline_release_remote: Option<String>,
    pub dry_run: bool,
    pub steps: Vec<String>,
}

pub async fn install_git_hooks() -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_git_hooks(&repo_root)?;
    println!("Configured local core.hooksPath to ops/git-hooks");
    Ok(0)
}

pub async fn init_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    setup_direct_repo(opts).await
}

pub async fn adopt_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    setup_direct_repo(opts).await
}

pub async fn set_repo_mode(mode: RepoMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    let hooks = match mode {
        RepoMode::Direct => HookMode::Enforce,
        RepoMode::Observed => HookMode::Advisory,
        RepoMode::Enforced => HookMode::Enforce,
    };
    write_jeryu_configs(
        &repo_root,
        JeryuConfigSpec {
            mode,
            hooks,
            namespace: "root",
            name: "unknown",
            branch: "main",
            protect_main: true,
            main_relay: false,
            offline_release_remote: None,
        },
    )?;
    configure_hook_mode(&repo_root, hooks, HookProfile::All)?;
    println!("Configured JeRyu repo mode: {mode:?}");
    Ok(0)
}

pub async fn hooks_status() -> Result<i32> {
    let repo_root = git_repo_root()?;
    let hooks_path =
        git_config_get(&repo_root, "core.hooksPath")?.unwrap_or_else(|| "(unset)".into());
    println!("core.hooksPath: {hooks_path}");
    println!(
        "jeryu hook dir: {}",
        repo_root.join(".jeryu/hooks").display()
    );
    Ok(0)
}

pub async fn hooks_enable(mode: HookMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_hook_mode(&repo_root, mode, HookProfile::All)?;
    println!("Configured local JeRyu hooks: {mode:?}");
    Ok(0)
}

pub async fn hooks_disable() -> Result<i32> {
    let repo_root = git_repo_root()?;
    unset_git_config(&repo_root, "core.hooksPath")?;
    println!("Disabled local JeRyu hooks for this checkout");
    Ok(0)
}

pub async fn hooks_install(profile: HookProfile, mode: HookMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_hook_mode(&repo_root, mode, profile)?;
    println!("Installed JeRyu hook profile {profile:?} in {mode:?} mode");
    Ok(0)
}
