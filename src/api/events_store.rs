use std::collections::VecDeque;

use super::TuiEvent;

/// Bounded in-memory ring buffer for TUI event timeline rendering.
/// Events are appended by the control plane and consumed by the
/// bottom event timeline widget.
pub struct EventStore {
    events: VecDeque<TuiEvent>,
    capacity: usize,
    next_seq: u64,
}

impl EventStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_seq: 1,
        }
    }

    /// Append a new event, assigning it the next sequence number.
    pub fn push(&mut self, mut event: TuiEvent) -> u64 {
        let seq = self.next_seq;
        event.seq = seq;
        self.next_seq += 1;

        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
        seq
    }

    /// All events in order (oldest first).
    pub fn all(&self) -> impl Iterator<Item = &TuiEvent> {
        self.events.iter()
    }

    /// Events since a given cursor (exclusive).
    pub fn since(&self, cursor: u64) -> impl Iterator<Item = &TuiEvent> {
        self.events.iter().filter(move |e| e.seq > cursor)
    }

    /// Most recent N events (newest first).
    pub fn recent(&self, n: usize) -> Vec<&TuiEvent> {
        self.events.iter().rev().take(n).collect()
    }

    /// Current cursor (sequence of the last event).
    pub fn cursor(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    /// Number of events stored.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new(1000)
    }
}
