//! Split Tuiwright proof crate.
//!
//! Keep `tests/tui_tuiwright.rs` as the full monolith until every assertion
//! in `tests/tuiwright/README.md` has a replacement here.

#[path = "tuiwright/harness.rs"]
mod harness;

#[path = "tuiwright/capture.rs"]
mod capture;
