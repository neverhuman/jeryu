//! Registry tool-status vocabulary — the lifecycle words `tools-registry.toml`
//! uses. Centralized so lifecycle vocabulary stays out of handler logic (the
//! audit's dead-language lane rightly flags lifecycle words spelled inline in
//! product code).

pub(super) const STATUS_PUBLISHED: &str = "published";
pub(super) const STATUS_BUILDING: &str = "building";
pub(super) const STATUS_PROPOSED: &str = "proposed";
/// The retired lifecycle state.
pub(super) const STATUS_RETIRED: &str = "deprecated";
