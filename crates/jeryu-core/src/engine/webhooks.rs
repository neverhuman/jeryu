//! Webhooks, delivery outbox, and app installations.

use chrono::Utc;
use uuid::Uuid;

use super::{ForgeCore, require_name};
use crate::errors::Result;
use crate::model::*;

impl ForgeCore {
    pub fn create_webhook(
        &self,
        owner: &str,
        repo: &str,
        request: CreateWebhookRequest,
    ) -> Result<Webhook> {
        self.ensure_repo_exists(owner, repo)?;
        require_name("webhook url", &request.config.url)?;
        let now = Utc::now();
        let hook = Webhook {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            name: request.name,
            active: request.active,
            events: request.events,
            config: request.config,
            created_at: now,
            updated_at: now,
        };
        self.state
            .write()
            .webhooks
            .entry((owner.to_string(), repo.to_string()))
            .or_default()
            .push(hook.clone());
        Ok(hook)
    }

    pub fn list_webhooks(&self, owner: &str, repo: &str) -> Result<Vec<Webhook>> {
        self.ensure_repo_exists(owner, repo)?;
        // No webhooks entry for the repo means none are registered; an empty
        // list is the intended value.
        Ok(self
            .state
            .read()
            .webhooks
            .get(&(owner.to_string(), repo.to_string()))
            .cloned()
            .unwrap_or_default())
    }

    pub fn list_webhook_deliveries(&self, owner: &str, repo: &str) -> Result<Vec<WebhookDelivery>> {
        self.ensure_repo_exists(owner, repo)?;
        Ok(self
            .state
            .read()
            .webhook_deliveries
            .iter()
            .filter(|delivery| delivery.owner == owner && delivery.repo == repo)
            .cloned()
            .collect())
    }

    pub fn app_installations(&self) -> InstallationList {
        InstallationList {
            total_count: 0,
            installations: Vec::new(),
        }
    }
}
