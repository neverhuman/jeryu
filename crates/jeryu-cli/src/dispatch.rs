//! Dispatch router: parsed [`Cli`] + [`ForgeClient`] -> rendered output + exit code.
//!
//! The router holds no business logic; it fans out to the thin command
//! adapters and maps a [`ClientError`] to a non-zero exit code with a stderr
//! line, so the CLI behaves like an operator tool (0 on success, 1 on a
//! client error).

use std::io::Write;

use crate::cli::{AutonomyCommands, Cli, Commands};
use crate::client::{ClientError, ForgeClient};
use crate::commands;

/// Dispatch a parsed command against a client, writing human/JSON output to
/// `out` and any error line to `err`. Returns the process exit code.
pub fn dispatch(
    cli: Cli,
    client: &dyn ForgeClient,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let owner = cli.owner;
    let json = cli.json;
    let result = match cli.command {
        Commands::Forge(cmd) => commands::forge::run(client, &owner, json, cmd, out),
        Commands::Ci(cmd) => commands::ci::run(client, json, cmd, out),
        Commands::Runner(cmd) => commands::runner::run(client, json, cmd, out),
        Commands::Proof(cmd) => commands::proof::run(client, json, cmd, out),
        Commands::Release { version } => commands::release::run(client, json, &version, out),
        Commands::Cache(cmd) => commands::cache::run(client, json, cmd, out),
        Commands::GhSetup(args) => commands::gh_setup::run(json, args, out),
        Commands::Autonomy(AutonomyCommands::Init(args)) => {
            commands::autonomy::run(json, args, out)
        }
        Commands::Onboard(args) => commands::onboard::run(json, args, out),
    };

    match result {
        Ok(()) => 0,
        Err(error) => {
            writeln!(err, "error: {error}").ok();
            exit_code(&error)
        }
    }
}

fn exit_code(error: &ClientError) -> i32 {
    match error {
        ClientError::NotFound(_) => 2,
        ClientError::Conflict(_) => 3,
        ClientError::Invalid(_) => 4,
        ClientError::NotWired(_) => 5,
    }
}
