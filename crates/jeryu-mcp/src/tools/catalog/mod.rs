//! The 16-tool catalog: kinds, definitions, input schemas, and argument normalization.
//!
//! The catalog uses repository and pull-request identifiers that match the local
//! GitHub-compatible API surface.

mod definitions;
mod input_schema;
mod kind;

pub(crate) use definitions::tool_definition;
#[allow(unused_imports)] // Preserves the historic `catalog::{ToolDefinition, ToolKind}` paths.
pub(crate) use kind::{ToolDefinition, ToolKind};

use crate::backend::ToolDescriptor;
use crate::tools::CATALOG;

/// Build every tool descriptor in `CATALOG` order.
pub(crate) fn catalog() -> Vec<ToolDescriptor> {
    CATALOG
        .iter()
        .filter_map(|id| tool_definition(id).map(|def| def.descriptor(id)))
        .collect()
}
