//! Runtime router: frame composition / render driver ([`render`]) and the
//! keyboard event-loop scaffold ([`input`]).

pub mod input;
pub mod render;

pub use input::{Flow, handle_key};
pub use render::{draw, render_once};
