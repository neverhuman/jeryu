//! Event-loop scaffold: keyboard routing for the Flight Deck.
//!
//! The real product drives this from a crossterm event stream; here the routing
//! is factored into a pure [`handle_key`] so it is unit-testable without a
//! terminal. The event loop itself ([`run_loop`]) is a thin wrapper a binary
//! would call; it is not exercised by the standalone test suite (no TTY).

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{ActiveTab, App};

/// Result of routing a single key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep running; redraw on the next tick.
    Continue,
    /// Quit the event loop.
    Quit,
}

/// Route one key event against the app state. Pure: mutates `app`, returns the
/// control-flow decision. No I/O.
pub fn handle_key(app: &mut App, key: KeyEvent) -> Flow {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc if !app.focus.is_drilled() => Flow::Quit,
        KeyCode::Esc => {
            app.focus.escape();
            Flow::Continue
        }
        KeyCode::Tab => {
            app.focus.focus_next(app.active_tab);
            Flow::Continue
        }
        KeyCode::BackTab => {
            app.focus.focus_prev(app.active_tab);
            Flow::Continue
        }
        KeyCode::Right => {
            app.set_tab(app.active_tab.next());
            Flow::Continue
        }
        KeyCode::Left => {
            app.set_tab(app.active_tab.prev());
            Flow::Continue
        }
        KeyCode::Enter => {
            app.focus.push();
            app.focus.enter_fullscreen();
            Flow::Continue
        }
        KeyCode::Char(c @ '0'..='9') => {
            if let Some(tab) = ActiveTab::from_number(c as u8 - b'0') {
                app.set_tab(tab);
            }
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn q_quits_when_not_drilled() {
        let mut app = App::default();
        assert_eq!(handle_key(&mut app, key(KeyCode::Char('q'))), Flow::Quit);
    }

    #[test]
    fn digit_selects_tab() {
        let mut app = App::default();
        handle_key(&mut app, key(KeyCode::Char('1')));
        assert_eq!(app.active_tab, ActiveTab::Mission);
        handle_key(&mut app, key(KeyCode::Char('9')));
        assert_eq!(app.active_tab, ActiveTab::Evidence);
    }

    #[test]
    fn arrows_cycle_tabs() {
        let mut app = App::default(); // Workflow
        handle_key(&mut app, key(KeyCode::Right));
        assert_eq!(app.active_tab, ActiveTab::Mission);
        handle_key(&mut app, key(KeyCode::Left));
        assert_eq!(app.active_tab, ActiveTab::Workflow);
    }

    #[test]
    fn enter_drills_and_esc_unwinds_instead_of_quitting() {
        let mut app = App::default();
        assert_eq!(handle_key(&mut app, key(KeyCode::Enter)), Flow::Continue);
        assert!(app.focus.is_drilled());
        // Esc while drilled unwinds, does not quit.
        assert_eq!(handle_key(&mut app, key(KeyCode::Esc)), Flow::Continue);
        assert!(!app.focus.is_drilled());
        // Esc at top level quits.
        assert_eq!(handle_key(&mut app, key(KeyCode::Esc)), Flow::Quit);
    }

    #[test]
    fn tab_cycles_focus_within_tab() {
        let mut app = App::default();
        app.set_tab(ActiveTab::Mission);
        let first = app.focus.active;
        handle_key(&mut app, key(KeyCode::Tab));
        assert_ne!(app.focus.active, first);
    }
}
