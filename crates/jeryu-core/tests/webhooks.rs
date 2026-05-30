//! Webhook configuration, signing, event filtering, and delivery-outbox tests.
//!
//! These assert the GitHub-shaped delivery envelope: an `event` name, the
//! per-hook `signature_256` (HMAC-SHA256, `sha256=` prefix), and an action
//! payload carrying the GitHub `action` verb.

use jeryu_core::{
    CreateIssueRequest, CreatePullRequestRequest, CreateRepositoryRequest, CreateUserRequest,
    CreateWebhookRequest, ForgeCore, ForgeError, WebhookConfig, WebhookEventEnvelope,
    sign_webhook_payload,
};
use uuid::Uuid;

fn core_with_repo() -> ForgeCore {
    let core = ForgeCore::new();
    core.create_user(CreateUserRequest {
        login: "alice".to_string(),
        ..Default::default()
    })
    .unwrap();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "jeryu".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    core
}

fn webhook(core: &ForgeCore, events: &[&str], secret: Option<&str>) {
    core.create_webhook(
        "alice",
        "jeryu",
        CreateWebhookRequest {
            name: "web".to_string(),
            active: true,
            events: events.iter().map(|e| e.to_string()).collect(),
            config: WebhookConfig {
                url: "https://hooks.example.test/jeryu".to_string(),
                content_type: "json".to_string(),
                secret: secret.map(str::to_string),
            },
        },
    )
    .unwrap();
}

fn open_issue(core: &ForgeCore) {
    core.create_issue(
        "alice",
        "jeryu",
        "alice",
        CreateIssueRequest {
            title: "bug".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------

#[test]
fn signature_is_hmac_sha256_with_prefix() {
    let sig = sign_webhook_payload("topsecret", br#"{"action":"opened"}"#);
    assert!(sig.starts_with("sha256="));
    // 32-byte digest hex-encoded => 64 chars after the prefix.
    assert_eq!(sig.len(), "sha256=".len() + 64);
}

#[test]
fn signature_changes_with_secret_and_payload() {
    let a = sign_webhook_payload("k1", b"payload");
    let b = sign_webhook_payload("k2", b"payload");
    let c = sign_webhook_payload("k1", b"different");
    assert_ne!(a, b);
    assert_ne!(a, c);
    // Deterministic for the same inputs.
    assert_eq!(a, sign_webhook_payload("k1", b"payload"));
}

#[test]
fn signature_matches_known_vector() {
    // Cross-check against a precomputed HMAC-SHA256 of "hi" keyed by "key".
    let sig = sign_webhook_payload("key", b"hi");
    let expected = {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = <Hmac<Sha256>>::new_from_slice(b"key").unwrap();
        mac.update(b"hi");
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    };
    assert_eq!(sig, expected);
}

// ---------------------------------------------------------------------------
// Webhook config defaults + validation
// ---------------------------------------------------------------------------

#[test]
fn webhook_defaults_match_github_shape() {
    let req = CreateWebhookRequest::default();
    assert_eq!(req.name, "web");
    assert!(req.active);
    assert_eq!(req.events, vec!["push".to_string(), "pull_request".to_string()]);
    assert_eq!(req.config.content_type, "json");
}

#[test]
fn create_webhook_stores_and_lists() {
    let core = core_with_repo();
    webhook(&core, &["issues"], Some("s3cr3t"));
    let hooks = core.list_webhooks("alice", "jeryu").unwrap();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].events, vec!["issues".to_string()]);
    assert!(hooks[0].active);
    assert_eq!(hooks[0].config.url, "https://hooks.example.test/jeryu");
}

#[test]
fn empty_webhook_url_is_rejected() {
    let core = core_with_repo();
    let err = core
        .create_webhook(
            "alice",
            "jeryu",
            CreateWebhookRequest {
                config: WebhookConfig {
                    url: String::new(),
                    content_type: "json".to_string(),
                    secret: None,
                },
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, ForgeError::Validation(_)));
}

#[test]
fn webhook_on_missing_repo_is_not_found() {
    let core = ForgeCore::new();
    let err = core
        .create_webhook("nobody", "void", CreateWebhookRequest::default())
        .unwrap_err();
    assert!(matches!(err, ForgeError::NotFound(_)));
}

// ---------------------------------------------------------------------------
// Delivery: only matching events, with envelope shape
// ---------------------------------------------------------------------------

#[test]
fn matching_event_produces_signed_delivery() {
    let core = core_with_repo();
    webhook(&core, &["issues"], Some("hooksecret"));
    open_issue(&core);

    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert_eq!(deliveries.len(), 1);
    let d = &deliveries[0];
    assert_eq!(d.event, "issues");
    assert_eq!(d.target_url, "https://hooks.example.test/jeryu");
    assert!(!d.delivered, "outbox entries start undelivered");

    // Envelope/payload carries the GitHub action verb and the issue object.
    assert_eq!(d.payload["action"], "opened");
    assert!(d.payload.get("issue").is_some());
    // GitHub `number` field present (NOT a legacy iid).
    assert_eq!(d.payload["issue"]["number"], 1);
    assert!(d.payload["issue"].get("iid").is_none());

    // Signature is present and verifies against the delivered payload bytes.
    let sig = d.signature_256.as_ref().expect("signed delivery");
    let bytes = serde_json::to_vec(&d.payload).unwrap();
    assert_eq!(*sig, sign_webhook_payload("hooksecret", &bytes));
}

#[test]
fn unsubscribed_event_is_not_delivered() {
    let core = core_with_repo();
    // Subscribe only to pull_request; an issue event must NOT be delivered.
    webhook(&core, &["pull_request"], Some("s"));
    open_issue(&core);
    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert!(deliveries.is_empty());
}

#[test]
fn wildcard_event_subscription_receives_everything() {
    let core = core_with_repo();
    webhook(&core, &["*"], None);
    open_issue(&core);
    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert!(!deliveries.is_empty());
    assert_eq!(deliveries[0].event, "issues");
    // No secret configured => no signature attached.
    assert!(deliveries[0].signature_256.is_none());
}

#[test]
fn pull_request_open_emits_pull_request_event() {
    let core = core_with_repo();
    webhook(&core, &["pull_request"], Some("s"));
    core.create_pull_request(
        "alice",
        "jeryu",
        "alice",
        CreatePullRequestRequest {
            title: "feat".to_string(),
            head: "feature".to_string(),
            base: "main".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].event, "pull_request");
    assert_eq!(deliveries[0].payload["action"], "opened");
    assert_eq!(deliveries[0].payload["pull_request"]["number"], 1);
}

#[test]
fn inactive_webhook_receives_no_deliveries() {
    let core = core_with_repo();
    core.create_webhook(
        "alice",
        "jeryu",
        CreateWebhookRequest {
            name: "web".to_string(),
            active: false,
            events: vec!["issues".to_string()],
            config: WebhookConfig {
                url: "https://hooks.example.test/jeryu".to_string(),
                content_type: "json".to_string(),
                secret: None,
            },
        },
    )
    .unwrap();
    open_issue(&core);
    let deliveries = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert!(deliveries.is_empty(), "inactive hooks must not fire");
}

#[test]
fn deliveries_are_scoped_per_repo() {
    let core = core_with_repo();
    core.create_repository(
        "alice",
        CreateRepositoryRequest {
            name: "other".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    webhook(&core, &["issues"], Some("s"));
    open_issue(&core);

    // The "other" repo has no hooks and no deliveries.
    let other = core.list_webhook_deliveries("alice", "other").unwrap();
    assert!(other.is_empty());
    let jeryu = core.list_webhook_deliveries("alice", "jeryu").unwrap();
    assert_eq!(jeryu.len(), 1);
}

// ---------------------------------------------------------------------------
// WebhookEventEnvelope shape (the public delivery struct)
// ---------------------------------------------------------------------------

#[test]
fn webhook_event_envelope_serializes_with_expected_keys() {
    let env = WebhookEventEnvelope {
        delivery_id: Uuid::new_v4(),
        event: "pull_request".to_string(),
        hook_id: Uuid::new_v4(),
        payload: serde_json::json!({"action": "opened"}),
    };
    let json = serde_json::to_value(&env).unwrap();
    assert!(json.get("delivery_id").is_some());
    assert!(json.get("event").is_some());
    assert!(json.get("hook_id").is_some());
    assert_eq!(json["payload"]["action"], "opened");

    // Round-trips losslessly.
    let back: WebhookEventEnvelope = serde_json::from_value(json).unwrap();
    assert_eq!(back, env);
}
