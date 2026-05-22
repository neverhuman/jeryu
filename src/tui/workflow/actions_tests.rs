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
    dispatch_key(&mut state, &snap, KeyCode::Char('B'));
    for c in ['b', 'a', 'd'] {
        assert!(dispatch_key(&mut state, &snap, KeyCode::Char(c)).is_none());
    }
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
