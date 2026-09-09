//! Queue lens — depth against runner-fleet capacity (headroom / utilization).

pub mod data;
pub mod view;

pub use data::QueueLensInput;
pub use view::draw;
