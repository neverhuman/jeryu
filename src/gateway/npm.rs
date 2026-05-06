//! Owner: Cache Gateway subsystem — npm registry proxy
//! Proof: `cargo nextest run -p jeryu -- gateway::npm`
//! Invariants: npm package cache entries preserve integrity metadata and namespace trust boundaries.
use super::{fetch_bytes_with_singleflight, singleflight::Singleflight};
use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct NpmAdapter {
    upstream_url: String,
    http_client: Client,
    fetch_coalescer: Arc<Singleflight<Result<Vec<u8>, String>>>,
}

impl NpmAdapter {
    pub fn new(upstream_url: &str) -> Self {
        Self {
            upstream_url: upstream_url.to_string(),
            http_client: super::oci::build_http_client(30),
            fetch_coalescer: Arc::new(Singleflight::new()),
        }
    }

    pub async fn fetch_package(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let key = format!("{}:{}", name, version);
        let url = format!("{}/{}/-/{}-{}.tgz", self.upstream_url, name, name, version);
        fetch_bytes_with_singleflight(
            &self.fetch_coalescer,
            &key,
            "npm package",
            "npm package",
            || self.http_client.get(&url).send(),
        )
        .await
    }
}
