//! Owner: Remote command wrappers
//! Proof: `cargo test -p jeryu -- remote`
//! Invariants: Remote commands orchestrate SSH and remote bootstrap from Rust only.

use anyhow::Result;

use crate::cli::{RemoteActionCommands, RemoteCommand};
use jeryu::remote::{self, RemoteAction, RemoteCommonOptions};

fn map_remote_command(cmd: RemoteCommand) -> (RemoteAction, RemoteCommonOptions) {
    let opts = RemoteCommonOptions {
        dry_run: cmd.dry_run,
        json: cmd.json,
        yes: cmd.yes,
        color: cmd.color,
        interactive: cmd.interactive,
        service_mode: cmd.service_mode,
        verbose: cmd.verbose,
    };
    let action = match cmd.action {
        RemoteActionCommands::Install {
            target,
            alias,
            setup_key,
            identity,
        } => RemoteAction::Install {
            target,
            alias,
            setup_key,
            identity,
        },
        RemoteActionCommands::Refresh { alias } => RemoteAction::Refresh { alias },
        RemoteActionCommands::Doctor { alias } => RemoteAction::Doctor { alias },
        RemoteActionCommands::Status { alias } => RemoteAction::Status { alias },
        RemoteActionCommands::Logs { alias } => RemoteAction::Logs { alias },
        RemoteActionCommands::Restart { alias } => RemoteAction::Restart { alias },
        RemoteActionCommands::Stop { alias } => RemoteAction::Stop { alias },
        RemoteActionCommands::Start { alias } => RemoteAction::Start { alias },
        RemoteActionCommands::Ssh { alias } => RemoteAction::Ssh { alias },
        RemoteActionCommands::Run { alias, command } => RemoteAction::Run { alias, command },
        RemoteActionCommands::Tunnel { alias } => RemoteAction::Tunnel { alias },
        RemoteActionCommands::Uninstall { alias } => RemoteAction::Uninstall { alias },
    };
    (action, opts)
}

pub(crate) async fn execute_remote_command(cmd: RemoteCommand) -> Result<i32> {
    let (action, opts) = map_remote_command(cmd);
    remote::execute_remote(action, opts).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu::install::{ColorMode, InteractiveMode};
    use jeryu::remote::ServiceMode;

    #[test]
    fn refresh_command_maps_to_refresh_action() {
        let (action, opts) = map_remote_command(RemoteCommand {
            dry_run: true,
            json: false,
            yes: false,
            color: ColorMode::Auto,
            interactive: InteractiveMode::Auto,
            service_mode: ServiceMode::Auto,
            verbose: false,
            action: RemoteActionCommands::Refresh {
                alias: "xbabe1".into(),
            },
        });

        assert!(matches!(action, RemoteAction::Refresh { alias } if alias == "xbabe1"));
        assert!(opts.dry_run);
        assert!(!opts.json);
    }
}
