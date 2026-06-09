//! Reusable ratatui widgets (pure render; no backend I/O).
//!
//! The deliverable subset of the source widget library: the header strip, the
//! bottom status strip, the virtualized table, and the live agent-terminal pane.

pub mod header;
pub mod status_strip;
pub mod terminal_pane;
pub mod virtual_table;

pub use header::{HeaderProps, StreamMode};
pub use status_strip::{FrameMetrics, KeyHint, StatusStripProps};
pub use virtual_table::{Column, Row, VirtualTableProps};
