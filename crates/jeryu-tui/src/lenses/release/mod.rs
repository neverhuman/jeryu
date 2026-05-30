//! Release lens — release-candidate status / SBOM / promotion, projected from
//! the read model's release dashboard.

pub mod data;
pub mod view;

pub use data::{ReleaseLensInput, ReleaseRow};
pub use view::draw;
