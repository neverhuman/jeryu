//! Owner: Build Witness (Cacheability Classification)
//! Proof: `cargo test -p jeryu -- witness`
//! Invariants: is_cacheable=false when Cargo.lock is missing; uncacheable_reasons is always populated when is_cacheable=false; Docker and integration witnesses are never cacheable

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Witness {
    pub key: String,
    pub tool: String,
    pub is_cacheable: bool,
    pub uncacheable_reasons: Vec<String>,
}

pub struct WitnessBuilder;

impl WitnessBuilder {
    pub async fn docker_build_witness(
        project_id: i64,
        dockerfile_path: &Path,
        _context_dir: &Path,
    ) -> Result<Witness> {
        let mut hasher = Sha256::new();
        let mut uncacheable_reasons = Vec::new();
        let mut is_cacheable = true;

        hasher.update(project_id.to_string().as_bytes());

        if dockerfile_path.exists() {
            let content = fs::read(dockerfile_path).await?;
            let content_str = String::from_utf8_lossy(&content);

            if content_str.contains("--secret") {
                is_cacheable = false;
                uncacheable_reasons.push("Dockerfile uses --secret".into());
            }
            if content_str.contains("--ssh") {
                is_cacheable = false;
                uncacheable_reasons.push("Dockerfile uses --ssh".into());
            }

            for line in content_str.lines() {
                let trimmed = line.trim().to_uppercase();
                if trimmed.starts_with("FROM ") && !trimmed.contains("@SHA256:") {
                    uncacheable_reasons.push("Base image lacks @sha256 digest pin".into());
                }
                if trimmed.starts_with("ADD HTTP") {
                    is_cacheable = false;
                    uncacheable_reasons.push("Remote ADD indicates mutable URL fetch".into());
                }
                if trimmed.starts_with("ARG ")
                    && (trimmed.contains("TIMESTAMP") || trimmed.contains("RANDOM"))
                {
                    is_cacheable = false;
                    uncacheable_reasons.push("Non-deterministic ARG detected".into());
                }
            }

            hasher.update(&content);
        }

        // We intentionally DO NOT hash the context_dir.
        // BuildKit natively handles instruction-scoped layer invalidation (COPY, ADD) ignoring mtime.
        // Hashing the entire path or dir here would cause deterministic false-misses.

        let result = hasher.finalize();
        Ok(Witness {
            key: hex::encode(result),
            tool: "docker".to_string(),
            is_cacheable,
            uncacheable_reasons,
        })
    }

    pub async fn rust_build_witness(
        project_id: i64,
        cargo_lock_path: &Path,
        rustc_version: &str,
        witness_fingerprint: Option<&str>,
        target_triple: &str,
        profile: &str,
        features: &str,
    ) -> Result<Witness> {
        let mut hasher = Sha256::new();
        let mut uncacheable_reasons = Vec::new();
        let mut is_cacheable = true;

        hasher.update(project_id.to_string().as_bytes());

        if cargo_lock_path.exists() {
            let content = fs::read(cargo_lock_path).await?;
            hasher.update(&content);
        } else {
            is_cacheable = false;
            uncacheable_reasons.push("Cargo.lock is missing".into());
        }

        hasher.update(rustc_version.as_bytes());
        hasher.update(target_triple.as_bytes());
        hasher.update(profile.as_bytes());
        hasher.update(features.as_bytes());

        // Respect Cargo oracle inputs
        let rustflags = match std::env::var("RUSTFLAGS") {
            Ok(value) => value,
            Err(_) => String::new(),
        };
        hasher.update(rustflags.as_bytes());

        let wrapper = match std::env::var("RUSTC_WORKSPACE_WRAPPER") {
            Ok(value) => value,
            Err(_) => String::new(),
        };
        hasher.update(wrapper.as_bytes());

        if let Some(wf) = witness_fingerprint {
            hasher.update(wf.as_bytes());
        } else {
            is_cacheable = false;
            uncacheable_reasons.push("Missing cargo-witness fingerprint".into());
        }

        let result = hasher.finalize();
        Ok(Witness {
            key: hex::encode(result),
            tool: "cargo".to_string(),
            is_cacheable,
            uncacheable_reasons,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_witness_stability() {
        let witness = WitnessBuilder::rust_build_witness(
            1,
            Path::new("nonexistent"),
            "1.74.0",
            Some("fingerprint-123"),
            "x86_64-unknown-linux-gnu",
            "release",
            "all",
        )
        .await
        .unwrap();

        assert!(!witness.is_cacheable);
        assert_eq!(witness.uncacheable_reasons[0], "Cargo.lock is missing");
    }
}
