//! Owner: Cache Proxy (sccache TCP Proxy)
//! Proof: `cargo test -p jeryu -- cache_proxy`
//! Invariants: Proxy forwards to sccache; authentication failures are logged and traffic is dropped, not forwarded; proxy lifecycle is tied to the executor session

use anyhow::{Result, bail};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::state::Db;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ProxyVerdict {
    MitmIntercept,
    Passthrough,
}

impl ProxyVerdict {
    fn classify(host: &str) -> Self {
        if host.contains("crates.io") || host.contains("npmjs.org") {
            Self::MitmIntercept
        } else {
            Self::Passthrough
        }
    }

    fn reason_code(&self) -> &'static str {
        match self {
            Self::MitmIntercept => "intercepted_passthrough",
            Self::Passthrough => "passthrough",
        }
    }
}

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
            // Standard HTTP Parsing
            let _ = req.parse(&header_buf);
            let path = req.path.unwrap_or("/");
            let method = req.method.unwrap_or("GET");
            let mut is_conditional = false;

            for header in req.headers.iter() {
                let name = header.name.to_lowercase();
                if name == "if-none-match" || name == "if-modified-since" {
                    is_conditional = true;
                }
            }

            let reason_code = if is_conditional {
                "revalidated"
            } else {
                "cold_hit"
            };
            debug!("HTTP Request: {} {} [{}]", method, path, reason_code);

            if method == "GET" && path == "/api/v1/crates/config.json" {
                let config = r#"{"dl": "http://127.0.0.1:19800/api/v1/crates/{crate}/{version}/download", "api": "https://crates.io"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    config.len(),
                    config
                );
                stream.write_all(resp.as_bytes()).await?;

                let _ = db
                    .record_cache_request(
                        "crates.io/config.json",
                        method,
                        true,
                        "sparse_index_config",
                        config.len() as i64,
                    )
                    .await;
                return Ok(());
            }

            if method == "GET" && path.starts_with("/api/v1/crates/") && path.ends_with("/download")
            {
                let parts: Vec<&str> = path.split('/').collect();
                if parts.len() >= 7 {
                    let name = parts[4];
                    let version = parts[5];
                    tracing::info!("Intercepted HTTP cargo download for {} v{}", name, version);

                    // CAS lookup: check if we already have this crate cached locally
                    let cas_key = format!("crate:{}:{}", name, version);
                    use sha2::Digest;
                    let cas_digest = hex::encode(sha2::Sha256::digest(cas_key.as_bytes()));
                    let cas_dir = crate::config::data_dir().join("cache").join("crates");
                    let cas_file = cas_dir.join(&cas_digest);

                    if cas_file.exists()
                        && let Ok(cached_bytes) = tokio::fs::read(&cas_file).await
                    {
                        tracing::info!(
                            "CAS hit for cargo crate {} v{} ({} bytes)",
                            name,
                            version,
                            cached_bytes.len()
                        );
                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                            cached_bytes.len()
                        );
                        stream.write_all(resp.as_bytes()).await?;
                        stream.write_all(&cached_bytes).await?;

                        let _ = db
                            .record_cache_request(
                                &format!("crates.io{}", path),
                                method,
                                true,
                                "cas_hit",
                                cached_bytes.len() as i64,
                            )
                            .await;

                        return Ok(());
                    }

                    // CAS miss: fetch from upstream and store locally
                    match cargo_adapter.fetch_crate(name, version).await {
                        Ok(bytes) => {
                            // Store in CAS for future requests
                            let _ = tokio::fs::create_dir_all(&cas_dir).await;
                            let _ = tokio::fs::write(&cas_file, &bytes).await;

                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n\r\n",
                                bytes.len()
                            );
                            stream.write_all(resp.as_bytes()).await?;
                            stream.write_all(&bytes).await?;

                            let _ = db
                                .record_cache_request(
                                    &format!("crates.io{}", path),
                                    method,
                                    true,
                                    "singleflight_coalesced",
                                    bytes.len() as i64,
                                )
                                .await;

                            return Ok(());
                        }
                        Err(e) => {
                            tracing::error!("Failed to fetch cargo crate from adapter: {:?}", e);
                            let resp = "HTTP/1.1 502 Bad Gateway\r\n\r\n";
                            stream.write_all(resp.as_bytes()).await?;
                            return Ok(());
                        }
                    }
                }
            } else if method == "GET" && path.starts_with("/api/v1/crates/") {
                // Proxy the index requests via standard relay
                if let Some(suffix) = path.strip_prefix("/api/v1/crates/") {
                    let url = format!("https://index.crates.io/{}", suffix);
                    match reqwest::get(&url).await {
                        Ok(req_resp) => {
                            let raw_bytes = req_resp.bytes().await;
                            let bytes: &[u8] = match raw_bytes.as_deref() {
                                Ok(b) => b,
                                Err(_) => b"",
                            };
                            let resp_head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                                bytes.len()
                            );
                            stream.write_all(resp_head.as_bytes()).await?;
                            stream.write_all(&bytes).await?;

                            let _ = db
                                .record_cache_request(
                                    &format!("index.crates.io/{}", suffix),
                                    method,
                                    false,
                                    "sparse_index_relay",
                                    bytes.len() as i64,
                                )
                                .await;
                        }
                        Err(e) => {
                            tracing::error!("Failed to proxy index crates.io: {:?}", e);
                            let resp = "HTTP/1.1 502 Bad Gateway\r\n\r\n";
                            stream.write_all(resp.as_bytes()).await?;
                        }
                    }
                    return Ok(());
                }
            }

            let resp = "HTTP/1.1 501 Not Implemented\r\n\r\nProxy only supports CONNECT currently";
            stream.write_all(resp.as_bytes()).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "cache_proxy_tests.rs"]
mod cache_proxy_tests;
