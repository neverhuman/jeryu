//! Owner: Interactive TUI subsystem - Flight Deck event streams
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Stream state exposes degraded and polling modes to the UI.

use crate::api::inspection::EventPage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    Live,
    Polling,
    LastKnown,
    Fixture,
}

impl StreamMode {
    pub fn badge(self) -> &'static str {
        match self {
            Self::Live => "LIVE",
            Self::Polling => "[poll]",
            Self::LastKnown => "LAST KNOWN",
            Self::Fixture => "FIXTURE",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamCursor {
    pub last_seen: u64,
}

impl StreamCursor {
    pub fn new(last_seen: u64) -> Self {
        Self { last_seen }
    }

    pub fn request_cursor(&self) -> u64 {
        self.last_seen
    }

    pub fn record_page(&mut self, page: &EventPage) {
        self.last_seen = self.last_seen.max(page.next_cursor);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamState {
    pub mode: StreamMode,
    pub cursor: StreamCursor,
    pub last_error: Option<String>,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            mode: StreamMode::Polling,
            cursor: StreamCursor::new(0),
            last_error: None,
        }
    }
}

impl StreamState {
    pub fn mark_live(&mut self) {
        self.mode = StreamMode::Live;
        self.last_error = None;
    }

    pub fn mark_polling(&mut self, message: impl Into<String>) {
        self.mode = StreamMode::Polling;
        self.last_error = Some(message.into());
    }

    pub fn record_page(&mut self, page: &EventPage) {
        self.cursor.record_page(page);
        if self.mode == StreamMode::LastKnown {
            self.mode = StreamMode::Polling;
        }
    }

    pub fn resume_query(&self) -> String {
        format!("cursor={}", self.cursor.request_cursor())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_records_monotonic_pages() {
        let mut cursor = StreamCursor::new(12);
        cursor.record_page(&EventPage::empty(9));
        assert_eq!(cursor.request_cursor(), 12);

        cursor.record_page(&EventPage {
            cursor: 12,
            next_cursor: 18,
            events: Vec::new(),
        });
        assert_eq!(cursor.request_cursor(), 18);
    }

    #[test]
    fn stream_state_tracks_polling_reason_and_resume_query() {
        let mut state = StreamState::default();
        state.mark_live();
        assert_eq!(state.mode, StreamMode::Live);

        state.mark_polling("http disconnected");
        assert_eq!(state.mode, StreamMode::Polling);
        assert_eq!(state.last_error.as_deref(), Some("http disconnected"));
        assert_eq!(state.resume_query(), "cursor=0");
    }
}
