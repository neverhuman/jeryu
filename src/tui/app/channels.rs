//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.

// TODO(U09-followup): Bundle the bare `*_rx`/`*_tx` channel pairs currently
// on `App` (in `state.rs`) into a typed `AppChannels` struct constructed in
// `builder.rs`. Per the U09 first-cut rules ("If channels are bare fields
// on App, leave them on App for now — do not extract artificially") this
// file is intentionally minimal until the followup unit lands.
