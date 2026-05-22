use anyhow::Result;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use crate::gateway::cargo::CargoAdapter;
use crate::state::Db;

pub(super) async fn handle_http_request(
    mut stream: TcpStream,
    db: Db,
    cargo_adapter: CargoAdapter,
    header_buf: Vec<u8>,
) -> Result<()> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

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
    tracing::debug!("HTTP Request: {} {} [{}]", method, path, reason_code);

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

    if method == "GET" && path.starts_with("/api/v1/crates/") && path.ends_with("/download") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 7 {
            let name = parts[4];
            let version = parts[5];
            tracing::info!("Intercepted HTTP cargo download for {} v{}", name, version);

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

            match cargo_adapter.fetch_crate(name, version).await {
                Ok(bytes) => {
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
    } else if method == "GET"
        && path.starts_with("/api/v1/crates/")
        && let Some(suffix) = path.strip_prefix("/api/v1/crates/")
    {
        let url = format!("https://index.crates.io/{}", suffix);
        match reqwest::get(&url).await {
            Ok(req_resp) => {
                let raw_bytes = req_resp.bytes().await;
                let bytes: &[u8] = match raw_bytes.as_deref() {
                    Ok(b) => b,
                    Err(_) => b"",
                };
                let resp_head =
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len());
                stream.write_all(resp_head.as_bytes()).await?;
                stream.write_all(bytes).await?;

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

    let resp = "HTTP/1.1 501 Not Implemented\r\n\r\nProxy only supports CONNECT currently";
    stream.write_all(resp.as_bytes()).await?;
    Ok(())
}
