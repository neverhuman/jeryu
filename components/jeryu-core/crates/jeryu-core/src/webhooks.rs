use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

use crate::model::Webhook;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookEventEnvelope {
    pub delivery_id: Uuid,
    pub event: String,
    pub hook_id: Uuid,
    pub payload: Value,
}

pub fn sign_webhook_payload(secret: &str, payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(payload);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

pub fn should_deliver(hook: &Webhook, event: &str) -> bool {
    hook.active
        && (hook.events.iter().any(|name| name == "*")
            || hook.events.iter().any(|name| name == event))
}

pub fn event_payload(action: &str, object_name: &str, object: Value) -> Value {
    json!({
        "action": action,
        object_name: object,
        "sender": {
            "login": "jeryu"
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_signature_is_stable() {
        let signature = sign_webhook_payload("secret", br#"{"ok":true}"#);
        assert!(signature.starts_with("sha256="));
        assert_eq!(signature.len(), "sha256=".len() + 64);
    }
}
