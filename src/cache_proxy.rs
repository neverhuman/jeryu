//! Owner: Cache Proxy (sccache TCP Proxy)
//! Proof: `cargo test -p jeryu -- cache_proxy`
//! Invariants: Proxy forwards to sccache; authentication failures are logged and traffic is dropped, not forwarded; proxy lifecycle is tied to the executor session

use anyhow::{Result, bail};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::state::Db;

#[path = "cache_proxy_policy.rs"]
mod cache_proxy_policy;
use cache_proxy_policy::ProxyVerdict;

#[path = "cache_proxy_http.rs"]
mod cache_proxy_http;
use cache_proxy_http::handle_http_request;

async fn relay_and_record(
    mut stream: TcpStream,
    mut remote: TcpStream,
    host_port: &str,
    verdict: ProxyVerdict,
    db: &Db,
) -> Result<()> {
    let (_up, down) = tokio::io::copy_bidirectional(&mut stream, &mut remote)
        .await
        .unwrap_or((0, 0));
    let _ = db
        .record_cache_request(
            host_port,
            "CONNECT",
            false, // hit: false since it's just a proxy relay
            verdict.reason_code(),
            down as i64,
        )
        .await;
    Ok(())
}

pub struct CacheProxy {
    port: u16,
    db: Db,
    cargo_adapter: crate::gateway::cargo::CargoAdapter,
}

impl CacheProxy {
    pub fn new(port: u16, db: Db) -> Self {
        Self {
            port,
            db,
            cargo_adapter: crate::gateway::cargo::CargoAdapter::new("https://static.crates.io"),
        }
    }

    pub async fn start(self: Arc<Self>) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!("warp-proxy TCP listener bound on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let db_clone = self.db.clone();
                    let cargo_adapter = self.cargo_adapter.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::handle_connection(stream, db_clone, cargo_adapter).await
                        {
                            debug!("Proxy connection error: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection in warp-proxy: {:?}", e);
                }
            }
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        db: Db,
        cargo_adapter: crate::gateway::cargo::CargoAdapter,
    ) -> Result<()> {
        let mut header_buf = Vec::with_capacity(4096);

        loop {
            let mut byte = [0u8; 1];
            let n = stream.read(&mut byte).await?;
            if n == 0 {
                return Ok(());
            }
            header_buf.push(byte[0]);
            if header_buf.ends_with(b"\r\n\r\n") {
                break;
            }
            if header_buf.len() > 8192 {
                bail!("Header too large");
            }
        }

        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let header_str = String::from_utf8_lossy(&header_buf);
        if header_str.starts_with("CONNECT") {
            let host_line = header_str.lines().next().unwrap_or("");
            let _ = req.parse(&header_buf);

            let host_port = if let Some(path) = req.path {
                String::from(path)
            } else {
                let parts: Vec<&str> = host_line.split_whitespace().collect();
                if parts.len() >= 2 {
                    parts[1].to_string()
                } else {
                    "".to_string()
                }
            };

            if host_port.is_empty() {
                bail!("Invalid CONNECT request");
            }

            let verdict = ProxyVerdict::classify(&host_port);
            stream
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await?;

            debug!("warp-proxy {:?}: {}", verdict, host_port);

            match tokio::net::TcpStream::connect(&host_port).await {
                Ok(remote_stream) => {
                    let _ = relay_and_record(stream, remote_stream, &host_port, verdict, &db).await;
                }
                Err(e) => {
                    warn!(
                        "warp-proxy failed to connect upstream to {}: {}",
                        host_port, e
                    );
                }
            }
        } else {
            handle_http_request(stream, db, cargo_adapter, header_buf).await?;
        }

        Ok(())
    }
}
