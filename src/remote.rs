//! Owner: Remote SSH install and day-two management UX
//! Proof: `cargo test -p jeryu -- remote`
//! Invariants: Remote install dry-runs stay side-effect free; network mutations happen only after confirmation.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fs;
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

use crate::install::{
    ColorMode, InteractiveMode, color_text, current_exe_string, expand_tilde,
    prompt_for_confirmation_with_message, render_plan_steps, should_colorize, status_label,
};

#[path = "remote_support.rs"]
mod support;
pub(crate) use support::*;

#[path = "remote_shell.rs"]
mod remote_shell;

const DEFAULT_REMOTE_PREFIX: &str = "~/.jeryu";
const DEFAULT_REMOTE_BIN: &str = "~/.jeryu/bin/jeryu";
const DEFAULT_HTTP_PORT: u16 = 8929;
const DEFAULT_SSH_PORT: u16 = 2224;
const DEFAULT_VAULT_PORT: u16 = 18200;
const DEFAULT_WEBHOOK_PORT: u16 = 9777;
const DEFAULT_SSH_PORT_NUMBER: u16 = 22;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum, Default)]
pub enum ServiceMode {
    #[default]
    Auto,
    User,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConnection {
    pub alias: String,
    pub target: String,
    pub ssh_port: u16,
    pub identity: Option<String>,
    pub remote_prefix: String,
    pub remote_bin: String,
    pub local_http_port: u16,
    pub local_ssh_port: u16,
    pub local_vault_port: u16,
    pub local_webhook_port: u16,
}

fn build_remote_connection(
    alias: String,
    target: String,
    ssh_port: u16,
    identity: Option<String>,
) -> RemoteConnection {
    RemoteConnection {
        alias,
        target,
        ssh_port,
        identity,
        remote_prefix: DEFAULT_REMOTE_PREFIX.into(),
        remote_bin: DEFAULT_REMOTE_BIN.into(),
        local_http_port: DEFAULT_HTTP_PORT,
        local_ssh_port: DEFAULT_SSH_PORT,
        local_vault_port: DEFAULT_VAULT_PORT,
        local_webhook_port: DEFAULT_WEBHOOK_PORT,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(flatten)]
    pub connection: RemoteConnection,
    pub created_at_utc: String,
    #[serde(default)]
    pub service_mode: ServiceMode,
}

impl std::ops::Deref for RemoteConfig {
    type Target = RemoteConnection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteCommonOptions {
    pub dry_run: bool,
    pub json: bool,
    pub yes: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub service_mode: ServiceMode,
    pub verbose: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemotePreflight {
    pub local_ssh: bool,
    pub local_ssh_keygen: bool,
    pub remote_os: Option<String>,
    pub remote_arch: Option<String>,
    pub remote_docker_ready: Option<bool>,
    pub remote_systemd_user: Option<bool>,
    pub remote_disk_free_gb: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStep {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub command: Option<String>,
    pub requires_network: bool,
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteInstallPlan {
    pub action: String,
    #[serde(flatten)]
    pub connection: RemoteConnection,
    #[serde(flatten)]
    pub options: RemoteCommonOptions,
    pub setup_key: bool,
    pub preflight: RemotePreflight,
    pub steps: Vec<RemoteStep>,
}

#[derive(Debug, Clone)]
pub enum RemoteAction {
    Install {
        target: String,
        alias: Option<String>,
        setup_key: bool,
        identity: Option<PathBuf>,
    },
    Targeted {
        alias: String,
        op: RemoteOperation,
    },
}

#[derive(Debug, Clone)]
pub enum RemoteOperation {
    Refresh,
    Doctor,
    Status,
    Logs,
    Restart,
    Stop,
    Start,
    Ssh,
    Run { command: Vec<String> },
    Tunnel,
    Uninstall,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteReport {
    pub alias: String,
    pub target: String,
    pub config_path: String,
    pub remote_prefix: String,
    pub remote_bin: String,
    pub installed: bool,
    pub service_active: bool,
    pub docker_ready: bool,
    pub version_output: Option<String>,
}

pub async fn execute_remote(action: RemoteAction, opts: RemoteCommonOptions) -> Result<i32> {
    match action {
        RemoteAction::Install {
            target,
            alias,
            setup_key,
            identity,
        } => {
            let alias = match alias {
                Some(alias) => alias,
                None => default_alias(&target),
            };
            let cfg = RemoteConfig {
                connection: build_remote_connection(
                    alias.clone(),
                    target,
                    DEFAULT_SSH_PORT_NUMBER,
                    identity.as_ref().map(|path| path.display().to_string()),
                ),
                created_at_utc: Utc::now().to_rfc3339(),
                service_mode: ServiceMode::Auto,
            };
            remote_install(cfg, setup_key, &opts).await
        }
        RemoteAction::Targeted { alias, op } => match op {
            RemoteOperation::Refresh => {
                with_loaded_remote_config(
                    alias,
                    |cfg| async move { remote_refresh(&cfg, &opts).await },
                )
                .await
            }
            RemoteOperation::Doctor => {
                with_loaded_remote_config(
                    alias,
                    |cfg| async move { remote_doctor(&cfg, &opts).await },
                )
                .await
            }
            RemoteOperation::Status => {
                with_loaded_remote_config(
                    alias,
                    |cfg| async move { remote_status(&cfg, &opts).await },
                )
                .await
            }
            RemoteOperation::Logs => {
                with_loaded_remote_config(
                    alias,
                    |cfg| async move { remote_logs(&cfg, &opts).await },
                )
                .await
            }
            RemoteOperation::Restart => {
                with_loaded_remote_config(alias, |cfg| async move {
                    remote_service(&cfg, "restart", &opts).await
                })
                .await
            }
            RemoteOperation::Stop => {
                with_loaded_remote_config(alias, |cfg| async move {
                    remote_service(&cfg, "stop", &opts).await
                })
                .await
            }
            RemoteOperation::Start => {
                with_loaded_remote_config(alias, |cfg| async move {
                    remote_service(&cfg, "start", &opts).await
                })
                .await
            }
            RemoteOperation::Ssh => {
                with_loaded_remote_config(alias, |cfg| async move { remote_ssh(&cfg, &opts).await })
                    .await
            }
            RemoteOperation::Run { command } => {
                with_loaded_remote_config(alias, |cfg| async move {
                    remote_run(&cfg, command, &opts).await
                })
                .await
            }
            RemoteOperation::Tunnel => {
                with_loaded_remote_config(
                    alias,
                    |cfg| async move { remote_tunnel(&cfg, &opts).await },
                )
                .await
            }
            RemoteOperation::Uninstall => {
                with_loaded_remote_config(alias, |cfg| async move {
                    remote_uninstall(&cfg, &opts).await
                })
                .await
            }
        },
    }
}

#[path = "remote_ops.rs"]
mod ops;
pub(crate) use ops::*;
