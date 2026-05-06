//! Owner: Cache Gateway subsystem — OCI image proxy
//! Proof: `cargo nextest run -p jeryu -- gateway::oci`
//! Invariants: OCI cache decisions keep digest identity and trust namespace separation intact.
use super::{fetch_bytes_with_singleflight, singleflight::Singleflight};
use anyhow::Result;
use reqwest::Client;
use std::{sync::Arc, time::Duration};

pub(crate) fn build_http_client(timeout_secs: u64) -> Client {
    match Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(client) => client,
        Err(_) => Client::new(),
    }
}

#[derive(Clone)]
pub struct OciAdapter {
    upstream_url: String,
    http_client: Client,
    fetch_coalescer: Arc<Singleflight<Result<Vec<u8>, String>>>,
}

impl OciAdapter {
    pub fn new(upstream_url: &str) -> Self {
        Self {
            upstream_url: upstream_url.to_string(),
            http_client: build_http_client(60),
            fetch_coalescer: Arc::new(Singleflight::new()),
        }
    }

    pub async fn fetch_blob(&self, repo: &str, digest: &str) -> Result<Vec<u8>> {
        let key = format!("{}:{}", repo, digest);
        let url = format!("{}/v2/{}/blobs/{}", self.upstream_url, repo, digest);
        fetch_bytes_with_singleflight(&self.fetch_coalescer, &key, "OCI blob", "OCI blob", || {
            self.http_client.get(&url).send()
        })
        .await
    }
}
