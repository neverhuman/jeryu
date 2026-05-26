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
}

impl Default for InspectionState {
    fn default() -> Self {
        Self::new(
            TuiReadModel::default(),
            RuntimeProfile::new("default", "sqlite", "kafka"),
        )
    }
}
