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
    dispatch_with_api_url_env(cli, client, out, err, || {
        std::env::var("JERYU_API_URL").ok()
    })
}

/// Dispatch with an injectable `JERYU_API_URL` source.
///
/// Tests use this to keep in-memory dispatch isolated from the host
/// environment while production dispatch still honors the real environment.
pub fn dispatch_with_api_url_env(
    cli: Cli,
    client: &dyn ForgeClient,
    out: &mut dyn Write,
    err: &mut dyn Write,
    api_url_env: impl FnOnce() -> Option<String>,
) -> i32 {
    let owner = cli.owner;
    let json = cli.json;
    let api_url = match cli.api_url {
        Some(api_url) if !api_url.trim().is_empty() => Some(api_url),
        _ => match api_url_env() {
            Some(api_url) if !api_url.trim().is_empty() => Some(api_url),
            _ => None,
        },
    };
    let result = match cli.command {
        Commands::Forge(cmd) => {
            commands::forge::run(client, api_url.as_deref(), &owner, json, cmd, out)
        }
        Commands::Ci(cmd) => commands::ci::run(client, json, cmd, out),
        Commands::Runner(cmd) => commands::runner::run(client, json, cmd, out),
        Commands::Agent(cmd) => commands::agent::run(client, json, api_url.as_deref(), cmd, out),
        Commands::Proof(cmd) => commands::proof::run(client, json, cmd, out),
        Commands::Release { version } => commands::release::run(client, json, &version, out),
        Commands::Cache(cmd) => commands::cache::run(client, json, cmd, out),
        Commands::Status => commands::control_plane::run_status(json, api_url.as_deref(), out),
        Commands::Priorities { limit } => {
            commands::control_plane::run_priorities(json, api_url.as_deref(), limit, out)
        }
        Commands::RepoGraph(cmd) => {
            commands::control_plane::run_repo_graph(json, api_url.as_deref(), cmd, out)
        }
        Commands::ToolFinder(cmd) => {
            commands::control_plane::run_tool_finder(json, api_url.as_deref(), cmd, out)
        }
        Commands::Artifacts(cmd) => {
            commands::control_plane::run_artifacts(json, api_url.as_deref(), cmd, out)
        }
        Commands::Runners(cmd) => {
            commands::control_plane::run_runners(json, api_url.as_deref(), cmd, out)
        }
        Commands::Serve { .. } => Ok(()),
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
