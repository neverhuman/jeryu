//! Evidence lens — proof receipts / gate decisions, projected from the read
//! model's evidence dashboard.

pub mod data;
pub mod view;

pub use data::{CodegraphEvidenceRow, EvidenceLensInput, EvidenceRow, ToolBuildOpportunityRow};
pub use view::draw;
