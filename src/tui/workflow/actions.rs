//! Owner: Interactive TUI subsystem — Delivery Mission Control action pane
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::actions`
//! Invariants: Pure event router + renderer. No DB, no git_host calls — the
//! pane emits [`DeliveryAction`] events that an outer layer dispatches.
//!
//! Wave 5 (Evidence Gate rollout) introduces an in-TUI 5-button action surface
//! so an operator interrupting a verdict no longer has to drop to a shell:
//!
//!   [A]pprove once · [B]lock verdict · [R]equest repair ·
//!   [F]reeze 24h    · [K]ill bell
//!
//! `Block` and `KillBell` both require a free-text reason; the pane gates the
//! action behind a `PendingInput` prompt and only emits the [`DeliveryAction`]
//! once the operator hits Enter on a non-empty buffer. Esc clears the prompt.

use chrono::{DateTime, Utc};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::DeliverySnapshot;
use crate::tui::theme::Theme;

// ─── Public action surface ─────────────────────────────────────────────────

/// A side-effect request emitted by the Mission Control action pane. The TUI
/// module itself never performs the action — `App` routes these to the
/// appropriate backend (`KillBell`, autonomy CLI, git_host adapter, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryAction {
    /// Approve the current verdict once (does not change the underlying
    /// autonomy policy).
    ApproveOnce { pr_idx: usize },
    /// Block the current verdict. A reason is mandatory and is surfaced in
    /// the audit ledger.
    BlockVerdict { pr_idx: usize, reason: String },
    /// Ask the review agent to attempt a repair of the merge-request.
    RequestRepair { pr_idx: usize },
    /// Freeze autonomy for `hours` (default 24).
    FreezeAutonomy { hours: u32 },
    /// Engage the kill bell. Reason is required and signed into the ledger.
    KillBell { reason: String },
}

/// Outcome of a previously-submitted [`DeliveryAction`] — surfaced under the
/// action pane so operators can see whether their last keypress took effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    /// Successfully handed off to the dispatcher.
    Submitted,
    /// Dispatcher rejected the request (string explains why).
    Failed(String),
    /// Operator dismissed the pending input before it dispatched.
    Cancelled,
}

/// Snapshot of the most-recent action attempt.
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub action: String,
    pub outcome: ActionOutcome,
    pub at: DateTime<Utc>,
}

/// In-progress text capture for an action that requires a reason.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingInput {
    pub action_kind: String,
    pub prompt: String,
    pub buffer: String,
}

/// Persisted state of the action pane (selection, prompt buffer, last result).
#[derive(Debug, Clone, Default)]
pub struct ActionPaneState {
    pub visible: bool,
    pub focused_action: usize,
    pub pending_input: Option<PendingInput>,
    pub last_result: Option<ActionResult>,
}

/// The 5 canonical action buttons. Order is significant — `focused_action`
/// is an index into this slice.
pub const ACTIONS: &[(KeyCode, &str, &str)] = &[
    (KeyCode::Char('A'), "Approve", "Approve this verdict once"),
    (
        KeyCode::Char('B'),
        "Block",
        "Block this verdict (reason required)",
    ),
    (KeyCode::Char('R'), "Repair", "Ask agent to repair MR"),
    (KeyCode::Char('F'), "Freeze", "Freeze autonomy 24h"),
    (
        KeyCode::Char('K'),
        "KillBell",
        "Engage kill bell (reason required)",
    ),
];

// ─── Key dispatch ─────────────────────────────────────────────────────────

/// Translate a keystroke into a [`DeliveryAction`] (when ready to dispatch)
/// or mutate `state` (focus change, pending-input edit). Returns
/// `Some(action)` only when the action is fully ready (e.g. Block has a
/// non-empty reason).
///
/// The pane is intentionally a small text editor when `pending_input` is set:
/// characters extend the buffer, Backspace shortens it, Enter submits, Esc
/// cancels. This keeps the surface free of any external editor dependency.
pub fn dispatch_key(
    state: &mut ActionPaneState,
    snapshot: &DeliverySnapshot,
    key: KeyCode,
) -> Option<DeliveryAction> {
    // 1. If we are mid-prompt for a Block/KillBell reason, route keys to
    //    the input editor first.
    if let Some(pending) = state.pending_input.clone() {
        return handle_pending_key(state, snapshot, &pending, key);
    }

    // 2. Otherwise treat the key as a button activation. Uppercase keys
    //    (A/B/R/F/K) trigger the matching action; arrow keys move focus.
    match key {
        KeyCode::Up | KeyCode::BackTab => {
            if state.focused_action == 0 {
                state.focused_action = ACTIONS.len() - 1;
            } else {
                state.focused_action -= 1;
            }
            None
        }
        KeyCode::Down | KeyCode::Tab => {
            state.focused_action = (state.focused_action + 1) % ACTIONS.len();
            None
        }
        KeyCode::Enter => trigger_action(state, snapshot, ACTIONS[state.focused_action].0),
        other => trigger_action(state, snapshot, other),
    }
}

fn trigger_action(
    state: &mut ActionPaneState,
    snapshot: &DeliverySnapshot,
    key: KeyCode,
) -> Option<DeliveryAction> {
    let pr_idx = snapshot.selected_pr_idx;
    match key {
        KeyCode::Char('A') => {
            mark_submitted(state, "Approve");
            focus_to(state, 0);
            Some(DeliveryAction::ApproveOnce { pr_idx })
        }
        KeyCode::Char('B') => {
            focus_to(state, 1);
            state.pending_input = Some(PendingInput {
                action_kind: "Block".into(),
                prompt: "Block reason (Enter to submit, Esc to cancel)".into(),
                buffer: String::new(),
            });
            None
        }
        KeyCode::Char('R') => {
            mark_submitted(state, "Repair");
            focus_to(state, 2);
            Some(DeliveryAction::RequestRepair { pr_idx })
        }
        KeyCode::Char('F') => {
            mark_submitted(state, "Freeze");
            focus_to(state, 3);
            Some(DeliveryAction::FreezeAutonomy { hours: 24 })
        }
        KeyCode::Char('K') => {
            focus_to(state, 4);
            state.pending_input = Some(PendingInput {
                action_kind: "KillBell".into(),
                prompt: "Kill-bell reason (Enter to engage, Esc to cancel)".into(),
                buffer: String::new(),
            });
            None
        }
        _ => None,
    }
}

fn handle_pending_key(
    state: &mut ActionPaneState,
    snapshot: &DeliverySnapshot,
    pending: &PendingInput,
    key: KeyCode,
) -> Option<DeliveryAction> {
    match key {
        KeyCode::Esc => {
            state.pending_input = None;
            state.last_result = Some(ActionResult {
                action: pending.action_kind.clone(),
                outcome: ActionOutcome::Cancelled,
                at: Utc::now(),
            });
            None
        }
        KeyCode::Backspace => {
            if let Some(p) = state.pending_input.as_mut() {
                p.buffer.pop();
            }
            None
        }
        KeyCode::Char(c) => {
            // Buffer cap keeps a stray key-repeat from blowing memory; 256
            // chars is plenty for a ledger reason.
            if let Some(p) = state.pending_input.as_mut()
                && p.buffer.len() < 256
            {
                p.buffer.push(c);
            }
            None
        }
        KeyCode::Enter => {
            let reason = pending.buffer.trim().to_string();
            if reason.is_empty() {
                // Stay in the prompt; nothing to dispatch yet.
                return None;
            }
            state.pending_input = None;
            mark_submitted(state, &pending.action_kind);
            match pending.action_kind.as_str() {
                "Block" => Some(DeliveryAction::BlockVerdict {
                    pr_idx: snapshot.selected_pr_idx,
                    reason,
                }),
                "KillBell" => Some(DeliveryAction::KillBell { reason }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn mark_submitted(state: &mut ActionPaneState, action: &str) {
    state.last_result = Some(ActionResult {
        action: action.to_string(),
        outcome: ActionOutcome::Submitted,
        at: Utc::now(),
    });
}

fn focus_to(state: &mut ActionPaneState, idx: usize) {
    if idx < ACTIONS.len() {
        state.focused_action = idx;
    }
}

// ─── Rendering ─────────────────────────────────────────────────────────────

/// Render the right-side action pane: vertical button list, then a prompt
/// row when `pending_input` is `Some`, then a last-result line.
pub fn render_action_pane(f: &mut Frame, area: Rect, state: &ActionPaneState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = Block::default()
        .title(" [ Mission Actions ] ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_active));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(ACTIONS.len() + 4);
    for (i, (key, label, help)) in ACTIONS.iter().enumerate() {
        let is_focused = state.focused_action == i;
        let hotkey = match key {
            KeyCode::Char(c) => *c,
            _ => '?',
        };
        let prefix_style = if is_focused {
            Style::default()
                .fg(theme.text_inverse)
                .bg(theme.selection)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.bold(theme.text_primary)
        };
        let label_style = if is_focused {
            theme.bold(theme.text_primary)
        } else {
            theme.secondary()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" [{}] ", hotkey), prefix_style),
            Span::styled((*label).to_string(), label_style),
        ]));
        lines.push(Line::from(vec![Span::styled(
            format!("     {}", help),
            theme.muted(),
        )]));
    }

    // Pending input row (when waiting on a reason).
    if let Some(p) = &state.pending_input {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            p.prompt.clone(),
            theme.bold(theme.warning),
        )));
        let mut buf = p.buffer.clone();
        buf.push('▌'); // caret
        lines.push(Line::from(Span::styled(
            format!("> {}", buf),
            theme.bold(theme.text_primary),
        )));
    }

    // Last-result row.
    if let Some(r) = &state.last_result {
        lines.push(Line::from(""));
        let (label, color) = match &r.outcome {
            ActionOutcome::Submitted => ("submitted".to_string(), theme.ok),
            ActionOutcome::Failed(msg) => (format!("failed: {}", msg), theme.fail),
            ActionOutcome::Cancelled => ("cancelled".to_string(), theme.text_muted),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{}: ", r.action), theme.bold(theme.text_primary)),
            Span::styled(label, theme.bold(color)),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn empty_snapshot() -> DeliverySnapshot {
        DeliverySnapshot::empty()
    }

    #[test]
    fn dispatch_key_a_returns_approve_action() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        let action = dispatch_key(&mut state, &snap, KeyCode::Char('A'));
        match action {
            Some(DeliveryAction::ApproveOnce { pr_idx }) => {
                assert_eq!(pr_idx, 0);
            }
            other => panic!("expected ApproveOnce, got {:?}", other),
        }
        assert!(state.pending_input.is_none());
        assert_eq!(state.focused_action, 0);
    }

    #[test]
    fn dispatch_key_b_enters_pending_input_for_reason() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        let action = dispatch_key(&mut state, &snap, KeyCode::Char('B'));
        assert!(action.is_none(), "Block must wait for a reason");
        let pending = state
            .pending_input
            .as_ref()
            .expect("pending input should be set");
        assert_eq!(pending.action_kind, "Block");
        assert!(pending.buffer.is_empty());
    }

    #[test]
    fn dispatch_key_b_after_reason_typed_returns_block_action() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        // Open the prompt.
        dispatch_key(&mut state, &snap, KeyCode::Char('B'));
        // Type "bad".
        for c in ['b', 'a', 'd'] {
            assert!(dispatch_key(&mut state, &snap, KeyCode::Char(c)).is_none());
        }
        // Submit.
        let action = dispatch_key(&mut state, &snap, KeyCode::Enter);
        match action {
            Some(DeliveryAction::BlockVerdict { pr_idx, reason }) => {
                assert_eq!(pr_idx, 0);
                assert_eq!(reason, "bad");
            }
            other => panic!("expected BlockVerdict, got {:?}", other),
        }
        assert!(state.pending_input.is_none());
        assert!(matches!(
            state.last_result.as_ref().map(|r| &r.outcome),
            Some(ActionOutcome::Submitted)
        ));
    }

    #[test]
    fn dispatch_key_esc_cancels_pending_input() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        dispatch_key(&mut state, &snap, KeyCode::Char('B'));
        assert!(state.pending_input.is_some());
        let action = dispatch_key(&mut state, &snap, KeyCode::Esc);
        assert!(action.is_none());
        assert!(state.pending_input.is_none());
        assert!(matches!(
            state.last_result.as_ref().map(|r| &r.outcome),
            Some(ActionOutcome::Cancelled)
        ));
    }

    #[test]
    fn dispatch_key_k_enters_pending_input_for_killbell_reason() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        let action = dispatch_key(&mut state, &snap, KeyCode::Char('K'));
        assert!(action.is_none());
        let pending = state.pending_input.expect("kill-bell prompt set");
        assert_eq!(pending.action_kind, "KillBell");
    }

    #[test]
    fn dispatch_key_empty_reason_does_not_dispatch() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        dispatch_key(&mut state, &snap, KeyCode::Char('K'));
        // Enter with empty buffer must stay in prompt.
        let action = dispatch_key(&mut state, &snap, KeyCode::Enter);
        assert!(action.is_none());
        assert!(
            state.pending_input.is_some(),
            "still waiting on a non-empty reason"
        );
    }

    #[test]
    fn dispatch_key_f_returns_freeze_with_24h_default() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        let action = dispatch_key(&mut state, &snap, KeyCode::Char('F'));
        assert_eq!(action, Some(DeliveryAction::FreezeAutonomy { hours: 24 }));
    }

    #[test]
    fn dispatch_key_r_returns_repair_action() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        let action = dispatch_key(&mut state, &snap, KeyCode::Char('R'));
        assert_eq!(action, Some(DeliveryAction::RequestRepair { pr_idx: 0 }));
    }

    #[test]
    fn dispatch_key_arrows_change_focus_without_dispatch() {
        let mut state = ActionPaneState::default();
        let snap = empty_snapshot();
        assert_eq!(state.focused_action, 0);
        assert!(dispatch_key(&mut state, &snap, KeyCode::Down).is_none());
        assert_eq!(state.focused_action, 1);
        assert!(dispatch_key(&mut state, &snap, KeyCode::Up).is_none());
        assert_eq!(state.focused_action, 0);
        assert!(dispatch_key(&mut state, &snap, KeyCode::Up).is_none());
        assert_eq!(state.focused_action, ACTIONS.len() - 1);
    }

    #[test]
    fn render_action_pane_includes_all_5_buttons() {
        let state = ActionPaneState {
            visible: true,
            ..Default::default()
        };
        let theme = Theme::dark();
        let backend = TestBackend::new(40, 30);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render_action_pane(f, f.area(), &state, &theme))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        for (_key, label, _help) in ACTIONS {
            assert!(
                rendered.contains(label),
                "action label {:?} should appear in rendered pane: {}",
                label,
                rendered
            );
        }
    }

    #[test]
    fn render_pending_prompt_shows_buffer_and_caret() {
        let mut state = ActionPaneState {
            visible: true,
            ..Default::default()
        };
        state.pending_input = Some(PendingInput {
            action_kind: "Block".into(),
            prompt: "Block reason".into(),
            buffer: "rollback".into(),
        });
        let theme = Theme::dark();
        let backend = TestBackend::new(40, 30);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render_action_pane(f, f.area(), &state, &theme))
            .unwrap();
        let buf = term.backend().buffer().clone();
        let mut rendered = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                rendered.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(rendered.contains("rollback"));
        assert!(rendered.contains("Block reason"));
    }
}
