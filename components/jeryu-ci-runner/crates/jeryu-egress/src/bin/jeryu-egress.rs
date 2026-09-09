#![doc = "Thin entrypoint for the host-allowlist egress proxy."]
#![doc = ""]
#![doc = "All policy lives in the `jeryu_egress` library; this binary only parses a"]
#![doc = "bind address from the environment and runs the proxy."]

use jeryu_egress::{Allowlist, Budget, Proxy};
use std::net::SocketAddr;

/// The default bind address (loopback, the port the legacy Python proxy used).
const DEFAULT_BIND: &str = "127.0.0.1:8889";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind: SocketAddr = std::env::var("JERYU_EGRESS_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("bad bind addr: {e}"),
            )
        })?;

    let proxy = Proxy::new(Allowlist::default(), Budget::new());
    proxy.serve(bind).await
}
