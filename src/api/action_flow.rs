use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{ActionResult, ActionStatus, ActorRef};
use crate::api::entity::EntityRef;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionOperationRequest {
    #[serde(default)]
    pub selected_entity: Option<EntityRef>,
    #[serde(default)]
    pub actor: Option<ActorRef>,
    #[serde(default)]
    pub grant_id: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub action_run_id: Option<String>,
    #[serde(default)]
    pub confirmation: Option<String>,
}

impl ActionOperationRequest {
    pub fn idempotency_key_or_run_id(&self) -> Option<&str> {
        self.idempotency_key
            .as_deref()
            .or(self.action_run_id.as_deref())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExecutionResponse {
    pub result: ActionResult,
    pub receipt: ActionReceipt,
    pub stream: ActionStreamPage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReceipt {
    pub receipt_id: String,
    pub action_id: String,
    pub idempotency_key: String,
    pub status: ActionStatus,
    pub dry_run: bool,
    pub summary: String,
    pub event_cursor: Option<u64>,
    pub affected_entity: Option<EntityRef>,
    pub evidence_created: Vec<String>,
    pub accepted_at: DateTime<Utc>,
}

impl ActionReceipt {
    pub fn from_result(
        action_id: &str,
        key: &str,
        dry_run: bool,
        result: &ActionResult,
        accepted_at: DateTime<Utc>,
    ) -> Self {
        Self {
            receipt_id: receipt_id(action_id, key),
            action_id: action_id.to_string(),
            idempotency_key: key.to_string(),
            status: result.status,
            dry_run,
            summary: result.summary.clone(),
            event_cursor: result.event_cursor,
            affected_entity: result.affected_entity.clone(),
            evidence_created: result.evidence_created.clone(),
            accepted_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStreamPage {
    pub cursor: u64,
    pub next_cursor: u64,
    pub events: Vec<ActionStreamEvent>,
}

impl ActionStreamPage {
    pub fn empty(cursor: u64) -> Self {
        Self {
            cursor,
            next_cursor: cursor,
            events: Vec::new(),
        }
    }

    pub fn single(event: ActionStreamEvent) -> Self {
        Self {
            cursor: event.seq.saturating_sub(1),
            next_cursor: event.seq,
            events: vec![event],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStreamEvent {
    pub seq: u64,
    pub action_id: String,
    pub phase: ActionStreamPhase,
    pub status: ActionStatus,
    pub summary: String,
    pub receipt_id: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStreamPhase {
    Preview,
    Execute,
    Cancel,
    Receipt,
}

pub fn receipt_id(action_id: &str, key: &str) -> String {
    format!("act_{action_id}_{:016x}", stable_hash(key.as_bytes()))
}

pub fn cursor_for_key(key: &str) -> u64 {
    stable_hash(key.as_bytes()).max(1)
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::actions::ActionStatus;

    #[test]
    fn receipt_ids_are_stable_for_idempotency_key() {
        assert_eq!(
            receipt_id("run_tests", "abc"),
            receipt_id("run_tests", "abc")
        );
        assert_ne!(
            receipt_id("run_tests", "abc"),
            receipt_id("run_tests", "def")
        );
    }

    #[test]
    fn stream_page_single_advances_cursor() {
        let event = ActionStreamEvent {
            seq: 42,
            action_id: "run_tests".into(),
            phase: ActionStreamPhase::Execute,
            status: ActionStatus::Accepted,
            summary: "accepted".into(),
            receipt_id: Some("r1".into()),
            timestamp: Utc::now(),
        };
        let page = ActionStreamPage::single(event);
        assert_eq!(page.cursor, 41);
        assert_eq!(page.next_cursor, 42);
        assert_eq!(page.events.len(), 1);
    }
}
