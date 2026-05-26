//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.

// TODO(U09-followup): Relocate `App::new`, `App::new_render_only`, and
// `App::build` from `src/tui/app_runtime.rs` into this file. The U09
// first-cut explicitly forbids touching `app_runtime.rs` (see
// `TUI_RESET_PLAN_FINAL.md` row TUI-RESET-20260526-011 + Hard rules) so
// the constructors stay where they live until the followup unit splits
// `app_runtime.rs` itself.
