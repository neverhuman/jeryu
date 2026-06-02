#[cfg(feature = "web")]
use std::net::SocketAddr;
#[cfg(feature = "web")]
use std::path::PathBuf;

#[cfg(feature = "web")]
use clap::{Parser, Subcommand};

#[cfg(feature = "web")]
#[derive(Debug, Parser)]
#[command(name = "jeryu-api")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[cfg(feature = "web")]
#[derive(Debug, Subcommand)]
enum Command {
    Web {
        #[command(subcommand)]
        command: WebCommand,
    },
}

#[cfg(feature = "web")]
#[derive(Debug, Subcommand)]
enum WebCommand {
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
        #[arg(long, default_value = "apps/web/dist")]
        spa_dir: PathBuf,
        #[arg(long, default_value = "~/.local/share/jeryu")]
        data_dir: PathBuf,
    },
}

#[cfg(feature = "web")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Web {
            command:
                WebCommand::Serve {
                    bind,
                    spa_dir,
                    data_dir,
                },
        }) => {
            let data_dir = expand_tilde(data_dir);
            let git_storage_root = data_dir.join("git");
            jeryu_api::web::serve(jeryu_api::web::WebServerConfig {
                bind,
                spa_dir,
                data_dir,
                git_storage_root,
            })
            .await
        }
        None => {
            let mut router = jeryu_api::Router::default();
            let response = router.get("/api/phase10/ready");
            println!("{} {}", response.status, response.body);
            Ok(())
        }
    }
}

#[cfg(feature = "web")]
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

#[cfg(not(feature = "web"))]
fn main() {
    let mut router = jeryu_api::Router::default();
    let response = router.get("/api/phase10/ready");
    println!("{} {}", response.status, response.body);
}
