//! Runtime router: frame composition / render driver ([`render`]) and the
//! keyboard event-loop scaffold ([`input`]).

pub mod input;
pub mod render;
pub mod tty;

pub use input::{Flow, handle_key, handle_key_with_sink, handle_terminal_key, pump_terminal};
pub use render::{draw, render_once};
pub use tty::{AgentControl, ControlSink, TtyChunk, TtySource};
