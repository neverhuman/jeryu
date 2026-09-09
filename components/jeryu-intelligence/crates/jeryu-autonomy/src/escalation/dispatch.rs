//! Fan-out dispatch: deliver an event to every matching webhook via the
//! [`EscalationSink`] seam, recording one [`DispatchResult`] per webhook and
//! never letting one webhook's failure abort the others.

use super::config::{EscalationConfig, EscalationKind};
use super::event::EscalationEvent;
use super::payload::build_payload;
use crate::seam::EscalationSink;

#[derive(Debug, Clone, PartialEq)]
pub struct DispatchResult {
    pub webhook_kind: EscalationKind,
    /// HTTP-like status code if a request fired. `None` on secret-resolution
    /// failure or transport-level error.
    pub status: Option<u16>,
    /// Human-readable error string. `None` on success.
    pub error: Option<String>,
}

impl DispatchResult {
    pub fn ok(kind: EscalationKind, status: u16) -> Self {
        Self {
            webhook_kind: kind,
            status: Some(status),
            error: None,
        }
    }
    pub fn err(kind: EscalationKind, status: Option<u16>, error: impl Into<String>) -> Self {
        Self {
            webhook_kind: kind,
            status,
            error: Some(error.into()),
        }
    }
}

/// Error type so we can distinguish "couldn't even start" from "started but
/// non-2xx".
#[derive(Debug, Clone, PartialEq)]
pub enum EscalationError {
    SecretMissing(String),
    Transport(String),
    HttpStatus { code: u16, body: String },
}

impl std::fmt::Display for EscalationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EscalationError::SecretMissing(name) => write!(f, "secret not resolvable: {name}"),
            EscalationError::Transport(s) => write!(f, "transport error: {s}"),
            EscalationError::HttpStatus { code, body } => {
                write!(f, "non-2xx status {code}: {body}")
            }
        }
    }
}

impl std::error::Error for EscalationError {}

/// Fan out the event to every configured webhook whose `on_events` allowlist
/// matches. Failures in one webhook do not abort the others.
pub async fn dispatch_all(
    config: &EscalationConfig,
    event: &EscalationEvent,
    sink: &dyn EscalationSink,
) -> Vec<DispatchResult> {
    if !config.permits(event.name()) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(config.webhooks.len());
    for wh in &config.webhooks {
        let payload = build_payload(event, wh.kind);
        let result = match sink.deliver(wh, payload).await {
            Ok(status) => DispatchResult::ok(wh.kind, status),
            Err(EscalationError::SecretMissing(name)) => {
                DispatchResult::err(wh.kind, None, format!("secret not resolvable: {name}"))
            }
            Err(EscalationError::Transport(msg)) => {
                DispatchResult::err(wh.kind, None, format!("transport error: {msg}"))
            }
            Err(EscalationError::HttpStatus { code, body }) => {
                DispatchResult::err(wh.kind, Some(code), format!("http {code}: {body}"))
            }
        };
        out.push(result);
    }
    out
}
