//! Inline available-action DTO shared by web summary contracts.
//!
//! Summaries advertise the actions the viewer may take (`available_actions`)
//! as an inline array of `{ action_id, label, risk }`. It is emitted inline at
//! each use site via a `#[ts(type = …)]` override, so this struct is a real
//! Rust source type without needing its own exported binding.

use serde::{Deserialize, Serialize};

/// One viewer-affordable action surfaced on a summary contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvailableAction {
    pub action_id: String,
    pub label: String,
    pub risk: Option<String>,
}
