//! Owner: TUI Control-Plane API - runtime profile contract
//! Proof: `cargo test -p jeryu --lib api::runtime_profile`
//! Invariants: profile output never exposes unredacted backend URLs or config paths.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeProfile {
    pub runtime_profile: String,
    pub state_backend: String,
    pub state_backend_url_redacted: Option<String>,
    pub broker_backend: String,
    pub build_sha: Option<String>,
    pub feature_flags: Vec<String>,
    pub schema_version: String,
    pub db_schema_hash: Option<String>,
    pub action_registry_hash: Option<String>,
    pub mcp_manifest_hash: Option<String>,
    pub inspection_api_version: String,
    pub config_paths_redacted: Vec<String>,
}

impl RuntimeProfile {
    pub fn new(
        runtime_profile: impl Into<String>,
        state_backend: impl Into<String>,
        broker_backend: impl Into<String>,
    ) -> Self {
        Self {
            runtime_profile: runtime_profile.into(),
            state_backend: state_backend.into(),
            state_backend_url_redacted: None,
            broker_backend: broker_backend.into(),
            build_sha: None,
            feature_flags: Vec::new(),
            schema_version: "unknown".into(),
            db_schema_hash: None,
            action_registry_hash: None,
            mcp_manifest_hash: None,
            inspection_api_version: "api.v1".into(),
            config_paths_redacted: Vec::new(),
        }
    }

    pub fn with_redacted_state_backend_url(mut self, url: &str) -> Self {
        self.state_backend_url_redacted = Some(redact_url(url));
        self
    }
}

fn redact_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://")
        && let Some((_, host)) = rest.rsplit_once('@')
    {
        return format!("{scheme}://***@{host}");
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_defaults_to_inspection_v1() {
        let profile = RuntimeProfile::new("dev", "sqlite", "kafka");
        assert_eq!(profile.inspection_api_version, "api.v1");
    }

    #[test]
    fn backend_url_credentials_are_redacted() {
        let profile = RuntimeProfile::new("prod", "redlinedb", "jansu")
            .with_redacted_state_backend_url("redline://user:secret@example/db");
        assert_eq!(
            profile.state_backend_url_redacted.as_deref(),
            Some("redline://***@example/db")
        );
    }
}
