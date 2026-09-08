//! Approvals lens — pending pull requests awaiting human review
//! (number/title/risk/CI checks/age), projected from the read model.

pub mod data;
pub mod view;

pub use data::{ApprovalRow, ApprovalsLensInput};
pub use view::draw;
