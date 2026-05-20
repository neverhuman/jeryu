//! Owner: Install command wrappers
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Install commands stay user-space by default and avoid shell scripts.

use anyhow::Result;

use crate::cli::{InstallActionCommands, InstallCommand};
use jeryu::install::{self, InstallOptions};
use jeryu::install_demo;

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
    match cmd.action {
        None => install::run_local(&opts).await,
        Some(InstallActionCommands::Guided) => install::run_guided(&opts).await,
        Some(InstallActionCommands::Doctor) => install::run_doctor(&opts).await,
        Some(InstallActionCommands::Smoke) => install::run_smoke(&opts).await,
        Some(InstallActionCommands::Server) => install::run_server(&opts).await,
        Some(InstallActionCommands::Uninstall) => install::run_uninstall(&opts).await,
        Some(InstallActionCommands::RenderDemo { output, png }) => {
            if opts.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "action": "render-demo",
                        "output": output,
                        "png": png,
                        "dry_run": opts.dry_run,
                    }))?
                );
            }
            if opts.dry_run {
                println!(
                    "dry-run: would render install demo GIF to {}",
                    output.display()
                );
                return Ok(0);
            }
            install_demo::render_install_demo(&install_demo::Args { output, png })?;
            println!("install demo rendered");
            Ok(0)
        }
    }
}
