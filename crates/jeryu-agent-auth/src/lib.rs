//! Portable auth bridge for agent-edit native CLIs.
//!
//! This crate copies only portable auth material into Jeryu-owned storage and
//! per-run homes. Receipts record paths, modes, and digests, never secret
//! values. Host-bound auth is denied with a typed repair surface.

mod error;
mod fs_receipt;
mod import;
mod materialize;
mod oauth;
mod types;

pub use error::{AgentAuthError, AgentAuthRepair};
pub use import::{doctor, import_from_host};
pub use materialize::materialize_run_home;
pub use oauth::{
    DeviceCredential, DeviceFlowEndpoints, DeviceFlowHttp, DeviceFlowStart, HttpTransport,
    UpstreamDeviceFlow, import_from_device, is_expiring, load_credential,
    materialize_run_credential, mint_run_credential, refresh_if_expiring,
};
pub use types::{
    AgentToolKind, AuthDoctorReport, AuthFileReceipt, AuthImportOptions, AuthImportReceipt,
    RunAuthReceipt,
};
