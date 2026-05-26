//! Owner: Inspection API - read-only TUI projection plane
//! Proof: `cargo test -p jeryu --lib inspection::`
//! Invariants: routes are read-only and expose typed `api::inspection` payloads.

pub mod actions;

use chrono::Utc;
use serde::Serialize;

use crate::api::entity::{EntityDetail, EntityKind, EntityRef};
use crate::api::freshness::{SourceFreshness, SourceKind};
use crate::api::inspection::{
    ActionRegistryDocument, DeepHealth, EventPage, InspectionEnvelope, ProofDetail,
};
use crate::api::read_model::{RepoFamilySummary, ReposSnapshot, SCHEMA_VERSION, TuiReadModel};
use crate::api::runtime_profile::RuntimeProfile;
use crate::tui::action_registry::REGISTRY;

pub const API_PREFIX: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionHttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl InspectionHttpResponse {
    pub(crate) fn json<T: Serialize>(status: u16, payload: &T) -> Self {
        match serde_json::to_string(payload) {
            Ok(body) => Self {
                status,
                content_type: "application/json",
                body,
            },
            Err(err) => Self {
                status: 500,
                content_type: "application/json",
                body: format!(
                    "{{\"error\":\"inspection serialization failed\",\"detail\":{}}}",
                    json_string(&err.to_string())
                ),
            },
        }
    }
}

pub fn handle_get(path: &str) -> Option<InspectionHttpResponse> {
    let path = path.split_once('?').map_or(path, |(head, _)| head);
    if !path.starts_with(API_PREFIX) {
        return None;
    }

    let now = Utc::now();
    let sources = inspection_sources(now);
    let response = match path {
        "/api/v1/read-model" => {
            let model = read_model_for_inspection(now);
            InspectionHttpResponse::json(200, &InspectionEnvelope::new(model, sources, now))
        }
        "/api/v1/repos" => InspectionHttpResponse::json(
            200,
            &InspectionEnvelope::new(repos_for_inspection(), sources, now),
        ),
        "/api/v1/families" => InspectionHttpResponse::json(
            200,
            &InspectionEnvelope::new(families_for_inspection(), sources, now),
        ),
        "/api/v1/events" => InspectionHttpResponse::json(
            200,
            &InspectionEnvelope::new(EventPage::empty(0), sources, now),
        ),
        "/api/v1/runtime" => {
            let profile = RuntimeProfile::new("default", "sqlite", "kafka")
                .with_inspection_defaults(SCHEMA_VERSION, registry_fingerprint());
            InspectionHttpResponse::json(200, &InspectionEnvelope::new(profile, sources, now))
        }
        "/api/v1/deep-health" => {
            let model = TuiReadModel::default();
            let health = DeepHealth::from_read_model(&model, sources.clone());
            InspectionHttpResponse::json(200, &InspectionEnvelope::new(health, sources, now))
        }
        "/api/v1/action-registry" => {
            let actions = REGISTRY.iter().map(|entry| entry.contract_json()).collect();
            InspectionHttpResponse::json(200, &ActionRegistryDocument::new(actions))
        }
        _ if actions::is_action_get(path) => actions::handle_get(path),
        _ => route_entity_or_proof(path, sources, now)?,
    };

    Some(response)
}

fn read_model_for_inspection(now: chrono::DateTime<Utc>) -> TuiReadModel {
    let mut model = TuiReadModel::default();
    model.generated_at = now;
    model.schema_version = SCHEMA_VERSION.into();
    model.repos = repos_for_inspection();
    model
}

fn families_for_inspection() -> Vec<RepoFamilySummary> {
    repos_for_inspection().families
}

fn repos_for_inspection() -> ReposSnapshot {
    let Some(root) = inspection_workspace_root() else {
        return ReposSnapshot::default();
    };
    match crate::repo_fleet::load_registry_from(&root) {
        Ok(registry) => ReposSnapshot::from_registry(
            crate::repo_fleet::registry_path_for(&root)
                .display()
                .to_string(),
            &registry,
        ),
        Err(_) => ReposSnapshot::default(),
    }
}

fn inspection_workspace_root() -> Option<std::path::PathBuf> {
    if let Ok(root) = std::env::var("JERYU_WORKSPACE_ROOT") {
        let root = std::path::PathBuf::from(root);
        if root.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH).exists() {
            return Some(root);
        }
    }

    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(crate::repo_fleet::DEFAULT_REGISTRY_PATH).exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn handle_post(path: &str, body: &[u8]) -> Option<InspectionHttpResponse> {
    let path = path.split_once('?').map_or(path, |(head, _)| head);
    if !path.starts_with(API_PREFIX) {
        return None;
    }
    actions::handle_post(path, body)
}

fn route_entity_or_proof(
    path: &str,
    sources: Vec<SourceFreshness>,
    now: chrono::DateTime<Utc>,
) -> Option<InspectionHttpResponse> {
    if let Some(id) = path
        .strip_prefix("/api/v1/proofs/")
        .filter(|id| !id.is_empty())
    {
        let proof = ProofDetail::unavailable(id, now);
        return Some(InspectionHttpResponse::json(
            200,
            &InspectionEnvelope::new(proof, sources, now),
        ));
    }

    let rest = path.strip_prefix("/api/v1/entities/")?;
    let (kind_segment, id) = rest.split_once('/')?;
    if id.is_empty() {
        return Some(not_found("entity id is required"));
    }
    let Some(kind) = parse_entity_kind(kind_segment) else {
        return Some(not_found("unknown entity kind"));
    };

    let mut detail = EntityDetail::default();
    detail.entity = EntityRef::new(kind, id);
    detail.state = "unknown".into();
    detail.summary = "No live entity projection is wired for this entity yet.".into();
    detail.last_updated = Some(now);
    detail.expires_after_ms = Some(5_000);

    Some(InspectionHttpResponse::json(
        200,
        &InspectionEnvelope::new(detail, sources, now),
    ))
}

fn not_found(message: &str) -> InspectionHttpResponse {
    InspectionHttpResponse::json(404, &serde_json::json!({ "error": message }))
}

fn parse_entity_kind(segment: &str) -> Option<EntityKind> {
    EntityKind::ALL
        .iter()
        .copied()
        .find(|kind| kind.route_segment() == segment || kind.label() == segment)
}

pub(crate) fn inspection_sources(now: chrono::DateTime<Utc>) -> Vec<SourceFreshness> {
    vec![
        SourceFreshness::live(SourceKind::InspectionHttp, now, "inspection-http"),
        SourceFreshness::live(SourceKind::ActionRegistry, now, registry_fingerprint()),
    ]
}

fn registry_fingerprint() -> String {
    format!("registry:{}", REGISTRY.len())
}

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"serialization error\"".into())
}

trait RuntimeProfileInspectionDefaults {
    fn with_inspection_defaults(
        self,
        schema_version: &str,
        action_registry_hash: String,
    ) -> RuntimeProfile;
}

impl RuntimeProfileInspectionDefaults for RuntimeProfile {
    fn with_inspection_defaults(
        mut self,
        schema_version: &str,
        action_registry_hash: String,
    ) -> RuntimeProfile {
        self.schema_version = schema_version.into();
        self.inspection_api_version = crate::api::inspection::INSPECTION_API_VERSION.into();
        self.action_registry_hash = Some(action_registry_hash);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_body(path: &str) -> serde_json::Value {
        let response = handle_get(path).expect("inspection route");
        assert_eq!(response.status, 200);
        serde_json::from_str(&response.body).expect("valid json")
    }

    #[test]
    fn ignores_non_api_paths() {
        assert!(handle_get("/health").is_none());
    }

    #[test]
    fn read_model_route_returns_versioned_snapshot() {
        let body = json_body("/api/v1/read-model");
        assert_eq!(body["api_version"], "api.v1");
        assert_eq!(body["data"]["schema_version"], SCHEMA_VERSION);
        assert_eq!(body["sources"][0]["source"], "inspection_http");
    }

    #[test]
    fn repos_and_families_routes_project_workspace_registry() {
        let _guard = crate::test_sync::PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let temp = tempfile::tempdir().expect("temp workspace");
        let registry_dir = temp.path().join(".jeryu");
        std::fs::create_dir_all(&registry_dir).expect("registry dir");
        std::fs::create_dir_all(temp.path().join("core")).expect("repo dir");
        std::fs::write(
            registry_dir.join("repos.toml"),
            format!(
                r#"
schema_version = "1"

[[repo]]
alias = "core"
slug = "neverhuman/jeryu"
provider = "github"
remote = "https://github.com/neverhuman/jeryu.git"
local_root = "{}"
default_branch = "main"
visibility = "private"
health_profile = "rust-workspace"
"#,
                temp.path().join("core").display()
            ),
        )
        .expect("registry");
        let previous = std::env::var("JERYU_WORKSPACE_ROOT").ok();
        // SAFETY: This test holds PATH_ENV_LOCK for the full mutation window,
        // so no same-crate test mutates or depends on process env concurrently.
        unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", temp.path()) };

        let repos = json_body("/api/v1/repos");
        let families = json_body("/api/v1/families");
        let read_model = json_body("/api/v1/read-model");

        assert_eq!(repos["data"]["repos"][0]["alias"], "core");
        assert_eq!(repos["data"]["repos"][0]["family"], "neverhuman");
        assert_eq!(families["data"][0]["name"], "neverhuman");
        assert_eq!(
            read_model["data"]["repos"]["repos"][0]["slug"],
            "neverhuman/jeryu"
        );

        match previous {
            // SAFETY: PATH_ENV_LOCK is still held while restoring the original
            // process env value captured before this test mutation.
            Some(value) => unsafe { std::env::set_var("JERYU_WORKSPACE_ROOT", value) },
            // SAFETY: PATH_ENV_LOCK is still held while restoring the original
            // absence of this process env value.
            None => unsafe { std::env::remove_var("JERYU_WORKSPACE_ROOT") },
        }
    }

    #[test]
    fn events_route_returns_empty_page() {
        let body = json_body("/api/v1/events?cursor=99");
        assert_eq!(body["data"]["cursor"], 0);
        assert_eq!(body["data"]["next_cursor"], 0);
        assert!(body["data"]["events"].as_array().unwrap().is_empty());
    }

    #[test]
    fn entity_route_accepts_route_segment_and_label() {
        let mr = json_body("/api/v1/entities/merge-requests/12");
        assert_eq!(mr["data"]["entity"]["kind"], "merge_request");
        assert_eq!(mr["data"]["entity"]["id"], "12");

        let job = json_body("/api/v1/entities/job/7");
        assert_eq!(job["data"]["entity"]["kind"], "job");
        assert_eq!(job["data"]["entity"]["id"], "7");
    }

    #[test]
    fn unknown_entity_kind_is_404() {
        let response = handle_get("/api/v1/entities/not-real/1").expect("inspection route");
        assert_eq!(response.status, 404);
        assert!(response.body.contains("unknown entity kind"));
    }

    #[test]
    fn proof_route_returns_unavailable_contract() {
        let body = json_body("/api/v1/proofs/proof-1");
        assert_eq!(body["data"]["proof_id"], "proof-1");
        assert_eq!(body["data"]["status"], "unknown");
    }

    #[test]
    fn runtime_route_reports_registry_fingerprint() {
        let body = json_body("/api/v1/runtime");
        assert_eq!(body["data"]["inspection_api_version"], "api.v1");
        assert_eq!(body["data"]["schema_version"], SCHEMA_VERSION);
        assert_eq!(
            body["data"]["action_registry_hash"],
            format!("registry:{}", REGISTRY.len())
        );
    }

    #[test]
    fn action_registry_route_exposes_risk_contracts() {
        let response = handle_get("/api/v1/action-registry").expect("inspection route");
        let body: serde_json::Value = serde_json::from_str(&response.body).unwrap();
        assert_eq!(body["api_version"], "api.v1");
        assert_eq!(body["action_count"], REGISTRY.len());
        assert!(
            body["actions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|action| action.get("action_risk_tier").is_some())
        );
    }

    #[test]
    fn deep_health_route_reports_sources() {
        let body = json_body("/api/v1/deep-health");
        assert_eq!(body["data"]["sources"][0]["source"], "inspection_http");
        assert!(body["data"]["components"].as_array().unwrap().len() >= 5);
    }
}
