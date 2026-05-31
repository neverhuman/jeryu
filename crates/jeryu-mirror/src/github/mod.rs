//! GitHub provider export ingestion: normalize a GitHub export (file or JSON
//! value) into Jeryu's typed [`crate::model::Archive`].

mod archive;
mod parsers;
mod value;

pub use archive::{archive_from_github_value, load_github_export};
