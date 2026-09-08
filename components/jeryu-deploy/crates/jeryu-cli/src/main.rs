//! The `jeryu` operator/agent CLI binary.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use jeryu_cli::{Cli, InMemoryClient, cli::Commands, dispatch};

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Commands::Serve {
        bind,
        spa_dir,
        data_dir,
        split_manifest,
    } = &cli.command
    {
        return match serve(
            *bind,
            spa_dir.clone(),
            data_dir.clone(),
            split_manifest.clone(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::from(1)
            }
        };
    }

    // The binary runs against the in-memory client; swapping in an
    // `jeryu-api`/`jeryu-core`-backed client uses the identical dispatch seam.
    let client = InMemoryClient::new();

    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();

    let code = dispatch(cli, &client, &mut out, &mut err);
    out.flush().ok();
    err.flush().ok();

    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

fn serve(
    bind: std::net::SocketAddr,
    spa_dir: PathBuf,
    data_dir: PathBuf,
    split_manifests: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = expand_tilde(data_dir);
    let git_storage_root = data_dir.join("git");
    let trust_local_dev = env_flag("JERYU_WEB_TRUST_LOCAL");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(jeryu_api::web::serve(jeryu_api::web::WebServerConfig {
        bind,
        spa_dir,
        data_dir,
        git_storage_root,
        split_manifests,
        auth_required: true,
        trust_local_dev,
        secure_cookies: !trust_local_dev,
    }))
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn expand_tilde(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    if raw == "~" {
        std::env::var_os("HOME").map_or(path, PathBuf::from)
    } else if let Some(rest) = raw.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(rest))
            .unwrap_or(path)
    } else {
        path
    }
}
