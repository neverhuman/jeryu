//! Inline entity-handle DTO shared by web summary contracts.
//!
//! Every web entity (repo, PR, issue, …) carries a `{ kind, id }` handle so
//! the SPA can route to a canonical detail view without re-deriving the type
//! from the URL. It is emitted inline (`{ kind: string; id: string }`) at each
//! use site via a `#[ts(type = …)]` override, so this struct is a real Rust
//! source type without needing its own exported binding.

use serde::{Deserialize, Serialize};

/// Stable `{ kind, id }` pointer to a domain entity on the web surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityHandle {
    pub kind: String,
    pub id: String,
}
