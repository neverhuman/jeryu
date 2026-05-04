//! Owner: Install command wrappers
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Install commands stay user-space by default and avoid shell scripts.

use anyhow::Result;

use crate::cli::{InstallActionCommands, InstallCommand};
use jeryu::install::{self, InstallAction, InstallOptions};

pub(crate) async fn execute_install_command(cmd: InstallCommand) -> Result<i32> {
    let opts = InstallOptions {
        prefix: cmd.prefix,
        dry_run: cmd.dry_run,
        json: cmd.json,
        yes: cmd.yes,
        color: cmd.color,
        interactive: cmd.interactive,
        path_mode: cmd.path_mode,
        verbose: cmd.verbose,
        install_deps: cmd.install_deps,
        allow_sudo: cmd.allow_sudo,
    };
    let action = match cmd.action {
        None => None,
        Some(InstallActionCommands::Doctor) => Some(InstallAction::Doctor),
        Some(InstallActionCommands::Smoke) => Some(InstallAction::Smoke),
        Some(InstallActionCommands::Server) => Some(InstallAction::Server),
        Some(InstallActionCommands::Uninstall) => Some(InstallAction::Uninstall),
        Some(InstallActionCommands::RenderDemo { output, png }) => {
            Some(InstallAction::RenderDemo { output, png })
        }
    };
    install::execute_install(action, opts).await
}
