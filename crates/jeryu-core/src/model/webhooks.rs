//! Webhooks, delivery records, and app installations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default = "default_webhook_content_type")]
    pub content_type: String,
    #[serde(default)]
    pub secret: Option<String>,
}

pub fn default_webhook_content_type() -> String {
    "json".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Webhook {
    pub id: Uuid,
    pub owner: String,
    pub repo: String,
    pub name: String,
    pub active: bool,
    pub events: Vec<String>,
    pub config: WebhookConfig,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateWebhookRequest {
    #[serde(default = "default_webhook_name")]
    pub name: String,
    #[serde(default = "default_true")]
    pub active: bool,
    #[serde(default = "default_webhook_events")]
    pub events: Vec<String>,
    pub config: WebhookConfig,
}

pub fn default_webhook_name() -> String {
    "web".to_string()
}

pub fn default_webhook_events() -> Vec<String> {
    vec!["push".to_string(), "pull_request".to_string()]
}

pub fn default_true() -> bool {
    true
}

impl Default for CreateWebhookRequest {
    fn default() -> Self {
        Self {
            name: default_webhook_name(),
            active: true,
            events: default_webhook_events(),
            config: WebhookConfig {
                url: String::new(),
                content_type: default_webhook_content_type(),
                secret: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub hook_id: Uuid,
    pub owner: String,
    pub repo: String,
    pub event: String,
    pub target_url: String,
    pub payload: Value,
    pub signature_256: Option<String>,
    pub delivered: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubAppInstallation {
    pub id: Uuid,
    pub account_login: String,
    pub repository_selection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallationList {
    pub total_count: usize,
    pub installations: Vec<GitHubAppInstallation>,
}
