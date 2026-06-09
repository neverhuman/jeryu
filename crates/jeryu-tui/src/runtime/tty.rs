//! Terminal transport seam for the live agent-TTY pane.
//!
//! The pane consumes decoded terminal bytes through [`TtySource`] and emits
//! operator intents through [`ControlSink`]. This keeps the render/input layer
//! transport-agnostic: the production `WsTtyBridge` plugs in behind these two
//! traits later (consuming `AgentTtyEvent` frames over the agent-stream
//! WebSocket and sending [`AgentControl`] back upstream), while the tests drive
//! the exact same surface with scripted fakes — no socket, no agent.
//!
//! The streamed unit upstream is `AgentTtyEvent` (raw bytes in `bytes_b64`,
//! scope `agent_run.{id}`); a bridge decodes the base64 payload into a
//! [`TtyChunk`] before handing it to [`TtySource::poll`].

/// One decoded chunk of terminal output, tagged with its monotonic sequence so
/// the consumer can detect gaps (a missed chunk flips the session to lagged and
/// asks for a [`AgentControl::Resync`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyChunk {
    pub chunk_seq: u64,
    pub bytes: Vec<u8>,
}

/// An operator intent sent back toward the running agent's terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentControl {
    /// Keystrokes encoded as terminal input bytes.
    Input(Vec<u8>),
    /// Ctrl-C — interrupt the foreground process (does not close the pane).
    Interrupt,
    /// The pane geometry changed; ask the agent PTY to match.
    Resize { cols: u16, rows: u16 },
    /// A gap was detected; request a full screen replay from the source.
    Resync,
}

/// A source of decoded terminal chunks for an attached agent run.
pub trait TtySource {
    /// Drain all chunks available since the last poll, in sequence order.
    fn poll(&mut self) -> Vec<TtyChunk>;
}

/// A sink for operator intents flowing back toward the agent terminal.
pub trait ControlSink {
    /// Send one control intent upstream.
    fn send(&mut self, control: AgentControl);
}

#[cfg(test)]
pub use fakes::{RecordingControlSink, ScriptedTtySource};

#[cfg(test)]
mod fakes {
    use super::{AgentControl, ControlSink, TtyChunk, TtySource};

    /// A [`TtySource`] that yields a fixed script of chunks, draining them in
    /// order across successive polls.
    pub struct ScriptedTtySource {
        chunks: std::collections::VecDeque<TtyChunk>,
    }

    impl ScriptedTtySource {
        pub fn new(chunks: Vec<TtyChunk>) -> Self {
            Self {
                chunks: chunks.into(),
            }
        }

        /// Whether every scripted chunk has been drained.
        pub fn is_drained(&self) -> bool {
            self.chunks.is_empty()
        }
    }

    impl TtySource for ScriptedTtySource {
        fn poll(&mut self) -> Vec<TtyChunk> {
            self.chunks.drain(..).collect()
        }
    }

    /// A [`ControlSink`] that records every intent it is handed.
    #[derive(Debug, Default)]
    pub struct RecordingControlSink {
        pub sent: Vec<AgentControl>,
    }

    impl RecordingControlSink {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn last(&self) -> Option<&AgentControl> {
            self.sent.last()
        }
    }

    impl ControlSink for RecordingControlSink {
        fn send(&mut self, control: AgentControl) {
            self.sent.push(control);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_source_drains_chunks_in_order() {
        let mut source = ScriptedTtySource::new(vec![
            TtyChunk {
                chunk_seq: 1,
                bytes: b"a".to_vec(),
            },
            TtyChunk {
                chunk_seq: 2,
                bytes: b"b".to_vec(),
            },
        ]);
        let drained = source.poll();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].chunk_seq, 1);
        assert_eq!(drained[1].chunk_seq, 2);
        assert!(source.is_drained());
        assert!(source.poll().is_empty());
    }

    #[test]
    fn recording_sink_captures_controls() {
        let mut sink = RecordingControlSink::new();
        sink.send(AgentControl::Interrupt);
        sink.send(AgentControl::Input(b"ls".to_vec()));
        assert_eq!(sink.sent.len(), 2);
        assert_eq!(sink.sent[0], AgentControl::Interrupt);
        assert_eq!(sink.last(), Some(&AgentControl::Input(b"ls".to_vec())));
    }
}
