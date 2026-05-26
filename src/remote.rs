//! Owner: Remote SSH install and day-two management UX
//! Proof: `cargo test -p jeryu -- remote`
//! Invariants: Remote install dry-runs stay side-effect free; network mutations happen only after confirmation.

use anyhow::{Context, Result, bail};
use std::future::Future;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use tokio::process::Command;

use crate::install::{
    color_text, current_exe_string, expand_tilde, prompt_for_confirmation_with_message,
    render_plan_steps, should_colorize, status_label,
};

#[path = "remote_types.rs"]
mod types;
pub use types::*;

#[path = "remote_support.rs"]
mod support;
pub(crate) use support::*;

#[path = "remote_shell.rs"]
mod remote_shell;
pub(crate) use remote_shell::{run_remote_shell, run_remote_shell_capture, run_remote_shell_status};

pub async fn execute_remote(action: RemoteAction, opts: RemoteCommonOptions) -> Result<i32> {
    match action {
        RemoteAction::Install {
            target,
            alias,
            setup_key,
            identity,
        } => {
            let cfg = build_default_config(target, alias, identity);
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
