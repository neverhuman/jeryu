//! Owner: Inspection HTTP plane - shared per-request state.
//! Proof: `cargo test -p jeryu --lib inspection::state`
//! Invariants: state is cheap to clone (Arc-wrapped); handlers never own
//!             mutable references to durable backing stores.

use std::sync::{Arc, RwLock};

use crate::api::read_model::TuiReadModel;
use crate::api::runtime_profile::RuntimeProfile;

/// Cheap to clone; backs every `/api/v1/*` read handler.
#[derive(Clone)]
pub struct InspectionState {
    inner: Arc<Inner>,
}

struct Inner {
    read_model: RwLock<TuiReadModel>,
    runtime_profile: RwLock<RuntimeProfile>,
}

impl InspectionState {
    pub fn new(read_model: TuiReadModel, runtime_profile: RuntimeProfile) -> Self {
        Self {
            inner: Arc::new(Inner {
                read_model: RwLock::new(read_model),
                runtime_profile: RwLock::new(runtime_profile),
            }),
        }
    }

    pub fn read_model(&self) -> TuiReadModel {
        self.inner
            .read_model
            .read()
            .expect("inspection rwlock poisoned")
            .clone()
    }

    pub fn replace_read_model(&self, next: TuiReadModel) {
        *self
            .inner
            .read_model
            .write()
            .expect("inspection rwlock poisoned") = next;
    }

    pub fn runtime_profile(&self) -> RuntimeProfile {
        self.inner
            .runtime_profile
            .read()
            .expect("inspection rwlock poisoned")
            .clone()
    }

    pub fn replace_runtime_profile(&self, next: RuntimeProfile) {
        *self
            .inner
            .runtime_profile
            .write()
            .expect("inspection rwlock poisoned") = next;
    }

    /// Snapshot the current set of `SourceFreshness` records the daemon
    /// is tracking. First-cut returns an empty `Vec`; real sources wire
    /// in via the daemon's projection loop (U07 follow-up, not in
    /// scope for the envelope adoption unit). Handlers attach this to
    /// every `InspectionEnvelope` so the wire shape is stable even
    /// before the freshness layer is online.
    pub fn snapshot_sources(&self) -> Vec<crate::api::freshness::SourceFreshness> {
        Vec::new()
    }
}

impl Default for InspectionState {
    fn default() -> Self {
        Self::new(
            TuiReadModel::default(),
            RuntimeProfile::new("default", "sqlite", "kafka"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_sources_returns_empty_until_projection_loop_lands() {
        let state = InspectionState::default();
        assert!(state.snapshot_sources().is_empty());
    }
}
