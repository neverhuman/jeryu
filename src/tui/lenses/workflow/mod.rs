//! Workflow lens facade for the Flight Deck reset.
//!
//! U19 keeps the existing delivery renderer stable while introducing the
//! split model and delivery import paths used by the new lens architecture.

pub mod canvas;
pub mod data;
pub mod delivery;
pub mod inspector;
pub mod model;
pub mod rails;
pub mod regions;
pub mod view;

pub use data::{WorkflowLensInput, select_workflow_lens_input};

#[cfg(test)]
mod tests;
