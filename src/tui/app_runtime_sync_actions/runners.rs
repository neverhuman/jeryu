//! Owner: Interactive TUI subsystem — runner feed controls
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: feed actions only mutate `active_feed_index`, scroll/follow
//! state and the pinned-feed marker — no I/O.

use super::*;

impl App {
    pub fn feed_next(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            self.state.active_feed_index =
                (self.state.active_feed_index + 1) % self.state.runner_feeds.len();
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_prev(&mut self) {
        if !self.state.runner_feeds.is_empty() {
            if self.state.active_feed_index > 0 {
                self.state.active_feed_index -= 1;
            } else {
                self.state.active_feed_index = self.state.runner_feeds.len() - 1;
            }
            self.feed_scroll_offset = 0;
            self.feed_follow_tail = true;
        }
    }

    pub fn feed_toggle_pin(&mut self) {
        if self.feed_pinned.is_some() {
            self.feed_pinned = None;
        } else {
            self.feed_pinned = Some(self.state.active_feed_index);
        }
    }

    pub fn feed_follow_toggle(&mut self) {
        self.feed_follow_tail = !self.feed_follow_tail;
        if self.feed_follow_tail {
            self.feed_scroll_offset = u16::MAX;
        }
    }
}
