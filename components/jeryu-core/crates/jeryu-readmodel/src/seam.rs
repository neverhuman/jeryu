//! Thin trait seam standing in for the future `jeryu-*` core.
//!
//! This crate is the immutable read-model CONTRACT. It must not depend on the
//! engine crates. The traits here describe the surface that a future core
//! (assembler / control plane) implements to *produce* the contract types —
//! the TUI and web edge depend only on the contract, and the daemon wires a
//! concrete backend behind these seams.
//!
//! All methods are synchronous and return owned contract types so the trait is
//! object-safe (`dyn ReadModelSource` / `dyn ToolBackend` / `dyn BugStore`) and
//! trivially mockable in tests. No async runtime is pulled in at the contract
//! layer; the live assembler can wrap these in async at the edge.

use crate::dashboards::runners::RunnersDashboard;
use crate::entity::{Bug, EntityDetail, EntityRef};
use crate::read_model::TuiReadModel;

/// Typed error returned by a read-model source when a panel cannot be
/// hydrated. Carries enough context for a degraded badge, never a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamError {
    pub source: &'static str,
    pub reason: String,
}

impl SeamError {
    pub fn new(source: &'static str, reason: impl Into<String>) -> Self {
        Self {
            source,
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for SeamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.source, self.reason)
    }
}

impl std::error::Error for SeamError {}

pub type SeamResult<T> = Result<T, SeamError>;

/// The single producing surface behind the read model. A future `jeryu-core`
/// assembler implements this; the TUI/web only ever consumes the returned
/// [`TuiReadModel`].
pub trait ReadModelSource {
    /// Assemble the full immutable snapshot for first paint / delta refresh.
    fn read_model(&self) -> SeamResult<TuiReadModel>;

    /// Fetch full inspector detail for a single entity (`/entity/{kind}/{id}`).
    fn entity_detail(&self, entity: &EntityRef) -> SeamResult<EntityDetail>;
}

/// Sandbox/runner controller seam (provider-neutral; replaces the source
/// product's docker controller). Supplies the runner fleet projection.
pub trait ToolBackend {
    /// Current runner/sandbox fleet dashboard.
    fn runners_dashboard(&self) -> SeamResult<RunnersDashboard>;

    /// Whether the control plane is reachable (drives the degraded badge).
    fn is_ready(&self) -> bool;
}

/// Bug/blocker store seam (RedlineDB-backed in the live product). Read-only at
/// the contract layer.
pub trait BugStore {
    /// List currently open bugs/blockers.
    fn open_bugs(&self) -> SeamResult<Vec<Bug>>;

    /// Fetch a single bug by id, if present.
    fn bug(&self, id: &str) -> SeamResult<Option<Bug>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityKind;

    // A trivial in-memory backend proving the seam is object-safe and usable.
    struct FixtureBackend {
        model: TuiReadModel,
        bugs: Vec<Bug>,
    }

    impl ReadModelSource for FixtureBackend {
        fn read_model(&self) -> SeamResult<TuiReadModel> {
            Ok(self.model.clone())
        }
        fn entity_detail(&self, entity: &EntityRef) -> SeamResult<EntityDetail> {
            Ok(EntityDetail {
                entity: entity.clone(),
                ..Default::default()
            })
        }
    }

    impl ToolBackend for FixtureBackend {
        fn runners_dashboard(&self) -> SeamResult<RunnersDashboard> {
            Ok(self.model.runners.clone())
        }
        fn is_ready(&self) -> bool {
            true
        }
    }

    impl BugStore for FixtureBackend {
        fn open_bugs(&self) -> SeamResult<Vec<Bug>> {
            Ok(self.bugs.clone())
        }
        fn bug(&self, id: &str) -> SeamResult<Option<Bug>> {
            Ok(self.bugs.iter().find(|b| b.id == id).cloned())
        }
    }

    #[test]
    fn seam_traits_are_object_safe_and_usable() {
        let backend = FixtureBackend {
            model: TuiReadModel::default(),
            bugs: Vec::new(),
        };
        let rm: &dyn ReadModelSource = &backend;
        let tb: &dyn ToolBackend = &backend;
        let bs: &dyn BugStore = &backend;

        assert_eq!(rm.read_model().unwrap().schema_version, "tui.v1.0");
        assert!(tb.is_ready());
        assert!(tb.runners_dashboard().unwrap().items.is_empty());
        assert!(bs.open_bugs().unwrap().is_empty());

        let detail = rm
            .entity_detail(&EntityRef::new(EntityKind::Job, "1"))
            .unwrap();
        assert_eq!(detail.entity.id, "1");
    }

    #[test]
    fn seam_error_displays() {
        let err = SeamError::new("scm", "timeout");
        assert_eq!(err.to_string(), "scm: timeout");
    }
}
