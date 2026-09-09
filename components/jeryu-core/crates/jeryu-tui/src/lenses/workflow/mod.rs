//! Workflow lens — delivery posture per repo, projected from the read model's
//! workflow dashboard (GitHub PR shape; pipelines keyed by PR number).

pub mod data;
pub mod view;

pub use data::{WorkflowLensInput, WorkflowRow};
pub use view::draw;
