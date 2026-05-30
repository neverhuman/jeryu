//! Mission lens — operational posture, attention queue, next-action.
//!
//! Canonical lens shape: `data` (pure projector) + `view` (pure renderer).

pub mod data;
pub mod view;

pub use data::MissionLensInput;
pub use view::draw;
