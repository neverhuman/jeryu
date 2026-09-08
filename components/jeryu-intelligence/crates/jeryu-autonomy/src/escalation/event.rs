//! The escalation events themselves (`RequireHuman`, `KillBellEngaged`) and
//! their stable wire names, human summaries, and self-describing JSON forms.

use crate::types::VibeGateVerdict;

#[derive(Debug, Clone)]
pub enum EscalationEvent {
    RequireHuman { verdict: Box<VibeGateVerdict> },
    KillBellEngaged { reason: String, paused_by: String },
}

impl EscalationEvent {
    /// Stable string used in `on_events` allowlists.
    pub fn name(&self) -> &'static str {
        match self {
            EscalationEvent::RequireHuman { .. } => "require_human",
            EscalationEvent::KillBellEngaged { .. } => "kill_bell_engaged",
        }
    }

    /// Short human-readable summary used in webhook message bodies.
    pub fn summary(&self) -> String {
        match self {
            EscalationEvent::RequireHuman { verdict } => format!(
                "[jeryu] RequireHuman on {repo} @ {head} (risk={risk:?}, verdict={vid})",
                repo = verdict.repo,
                head = short_sha(&verdict.head_sha),
                risk = verdict.risk,
                vid = verdict.id,
            ),
            EscalationEvent::KillBellEngaged { reason, paused_by } => {
                format!("[jeryu] KillBellEngaged by {paused_by}: {reason}")
            }
        }
    }

    /// Self-describing JSON form.
    pub fn as_json(&self) -> serde_json::Value {
        match self {
            EscalationEvent::RequireHuman { verdict } => serde_json::json!({
                "event": "require_human",
                "verdict": verdict,
            }),
            EscalationEvent::KillBellEngaged { reason, paused_by } => serde_json::json!({
                "event": "kill_bell_engaged",
                "reason": reason,
                "paused_by": paused_by,
            }),
        }
    }
}

fn short_sha(sha: &str) -> &str {
    if sha.len() >= 7 { &sha[..7] } else { sha }
}
