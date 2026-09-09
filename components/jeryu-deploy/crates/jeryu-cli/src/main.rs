//! The `jeryu` operator/agent CLI binary.

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use jeryu_cli::{Cli, cli::Commands, client::RemoteOnlyClient, dispatch};

fn main() -> ExitCode {
    let mut cli = Cli::parse();
    if let Commands::Serve {
        bind,
        spa_dir,
        data_dir,
        store,
        split_manifest,
    } = &cli.command
    {
        return match serve(
            *bind,
            spa_dir.clone().unwrap_or_default(),
            data_dir.clone(),
            store.clone(),
            split_manifest.clone(),
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::from(1)
            }
        };
    }

    cli.api_url = Some(cli.api_url.unwrap_or_else(|| {
        std::env::var("JERYU_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into())
    }));
    let client = RemoteOnlyClient;

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
    data_dir: Option<PathBuf>,
    store: Option<String>,
    split_manifests: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let env_store = std::env::var("JERYU_STORE").ok();
    let resolved = jeryu_cli::store::resolve(store.as_deref().or(env_store.as_deref()))?;
    if let Some(notice) = resolved.fallback_notice() {
        eprintln!("{notice}");
    }
    let data_dir = jeryu_cli::data_dir::resolve(data_dir)?;
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
        secure_cookies: !bind.ip().is_loopback(),
    }))
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}
