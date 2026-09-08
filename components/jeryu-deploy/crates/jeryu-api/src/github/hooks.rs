//! Webhook routes (`/repos/{owner}/{repo}/hooks`) and their GitHub-shaped
//! renderer.

use jeryu_core::{CreateWebhookRequest, Webhook};
use serde_json::{Value, json};

use crate::routes::Response;

use super::GithubRouter;
use super::support::{error_response, json_response, parse_body};

impl GithubRouter {
    pub(super) fn list_hooks(&self, owner: &str, repo: &str) -> Response {
        match self.core.list_webhooks(owner, repo) {
            Ok(hooks) => {
                let body: Vec<Value> = hooks.iter().map(webhook_json).collect();
                json_response(200, &Value::Array(body))
            }
            Err(err) => error_response(err),
        }
    }

    pub(super) fn create_hook(&self, owner: &str, repo: &str, body: &str) -> Response {
        let req: CreateWebhookRequest = match parse_body(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        match self.core.create_webhook(owner, repo, req) {
            Ok(hook) => json_response(201, &webhook_json(&hook)),
            Err(err) => error_response(err),
        }
    }
}

fn webhook_json(hook: &Webhook) -> Value {
    json!({
        "id": hook.id,
        "type": "Repository",
        "name": hook.name,
        "active": hook.active,
        "events": hook.events,
        "config": {
            "url": hook.config.url,
            "content_type": hook.config.content_type,
        },
        "url": format!("/repos/{}/{}/hooks/{}", hook.owner, hook.repo, hook.id),
        "created_at": hook.created_at,
        "updated_at": hook.updated_at,
    })
}
