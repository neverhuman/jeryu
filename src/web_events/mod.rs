//! Web Forge real-time event bus.
//!
//! Wire types live in `crate::api::websocket` (W-F-03). This module owns the
//! SERVER-side infrastructure: bounded broadcast bus, `TuiEvent -> WebEvent`
//! projection, and per-connection subscription state. The Axum WebSocket
//! handler in `crate::web::ws` (W-B-04) consumes these primitives.
//!
//! Backpressure policy per WEB_WORK_CLAUDE.md §35.1.13: bounded channel
//! (default 4096), priority classes never drop action results, audit/security
//! events, or direct mutation receipts.
//!
//! Heartbeat policy per §35.1.12: server pings every 15 s with a 30 s read
//! timeout enforced by the handler.

pub mod bus;
pub mod projection;
pub mod protocol;
pub mod subscription;

pub use bus::WebEventBus;
pub use projection::tui_event_to_web_event;
pub use subscription::{ConnectionId, SubscriptionRegistry};
