use crate::install::{ColorMode, InteractiveMode};
use chrono::Utc;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::support::default_alias;

const DEFAULT_REMOTE_PREFIX: &str = "~/.jeryu";
const DEFAULT_REMOTE_BIN: &str = "~/.jeryu/bin/jeryu";
pub(crate) const DEFAULT_HTTP_PORT: u16 = 8929;
pub(crate) const DEFAULT_SSH_PORT: u16 = 2224;
pub(crate) const DEFAULT_VAULT_PORT: u16 = 18200;
pub(crate) const DEFAULT_WEBHOOK_PORT: u16 = 9777;
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

pub(crate) fn build_remote_connection(
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

pub(crate) fn build_default_config(
    target: String,
    alias: Option<String>,
    identity: Option<PathBuf>,
) -> RemoteConfig {
    let alias = match alias {
        Some(alias) => alias,
        None => default_alias(&target),
    };
    RemoteConfig {
        connection: build_remote_connection(
            alias,
            target,
            DEFAULT_SSH_PORT_NUMBER,
            identity.map(|path| path.display().to_string()),
        ),
        created_at_utc: Utc::now().to_rfc3339(),
        service_mode: ServiceMode::Auto,
    }
}
