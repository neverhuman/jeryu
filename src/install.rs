//! Owner: Local installer and guided bootstrap UX
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Local installs remain user-space by default, avoid shell mutations unless requested, and never require sudo for the default path.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::ValueEnum;
use serde::Serialize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tempfile::Builder;
use tokio::process::Command;

#[path = "install_runtime.rs"]
mod install_runtime;

const JERYU_PATH_START: &str = "# >>> jeryu path >>>";
const JERYU_PATH_END: &str = "# <<< jeryu path <<<";

#[path = "install_support.rs"]
mod install_support;
pub(crate) use install_support::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum InteractiveMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum PathMode {
    Advise,
    Refresh,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformProbe {
    pub os: String,
    pub arch: String,
    pub shell: Option<String>,
    pub tty: bool,
    pub in_path: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathAdvice {
    pub shell: Option<String>,
    pub rc_file: Option<String>,
    pub snippet: Option<String>,
    pub refresh_performed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub action: String,
    pub mode: String,
    pub prefix: String,
    pub target_binary: String,
    pub source_binary: String,
    pub platform: PlatformProbe,
    pub path_advice: Option<PathAdvice>,
    pub dry_run: bool,
    pub json: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub path_mode: PathMode,
    pub verbose: bool,
    pub install_deps: bool,
    pub allow_sudo: bool,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub command: Option<String>,
    pub requires_sudo: bool,
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub prefix: String,
    pub binary: String,
    pub current_exe: String,
    pub installed: bool,
    pub version_ok: bool,
    pub version_output: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UninstallReport {
    pub action: String,
    pub prefix: String,
    pub binary: String,
    pub backup_dir: String,
    pub dry_run: bool,
    pub path_mode: PathMode,
    pub path_rc_file: Option<String>,
    pub binary_present_before: bool,
    pub backups_present_before: bool,
    pub path_block_found: bool,
    pub binary_removed: bool,
    pub backups_removed: bool,
    pub path_block_removed: bool,
}

/// Resolved install runtime options.
///
/// Field order here is grouped by concern (target, safety gates, output mode,
/// UX) and intentionally diverges from the flat clap layout in
/// `crate::cli::InstallCommand`. Initialise by name; clap is the only
/// canonical source for default values and CLI ergonomics.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    // --- target ---
    /// Install prefix; expands `~` against the current user.
    pub prefix: PathBuf,
    /// Strategy for managing the user's PATH (advise, refresh, skip).
    pub path_mode: PathMode,
    // --- safety gates ---
    /// Plan only; do not mutate the host filesystem.
    pub dry_run: bool,
    /// Skip interactive confirmation (`--yes`).
    pub yes: bool,
    /// Allow installing system-level dependencies.
    pub install_deps: bool,
    /// Permit invoking `sudo` for privileged steps.
    pub allow_sudo: bool,
    // --- output / UX ---
    /// Emit machine-readable JSON instead of human prose.
    pub json: bool,
    /// Verbose progress logging.
    pub verbose: bool,
    /// Color rendering policy.
    pub color: ColorMode,
    /// Interactive prompt policy.
    pub interactive: InteractiveMode,
}

pub async fn run_local(opts: &InstallOptions) -> Result<i32> {
    install_local(opts).await
}

pub async fn run_doctor(opts: &InstallOptions) -> Result<i32> {
    doctor(opts).await
}

pub async fn run_smoke(opts: &InstallOptions) -> Result<i32> {
    smoke(opts).await
}

pub async fn run_server(opts: &InstallOptions) -> Result<i32> {
    server(opts).await
}

pub async fn run_guided(opts: &InstallOptions) -> Result<i32> {
    guided(opts).await
}

pub async fn run_uninstall(opts: &InstallOptions) -> Result<i32> {
    uninstall(opts).await
}

pub fn expand_tilde(input: impl AsRef<str>) -> PathBuf {
    let input = input.as_ref();
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(input)
}
#[path = "install_commands.rs"]
mod install_commands;

pub(crate) use install_commands::{doctor, guided, install_local, server, smoke, uninstall};
