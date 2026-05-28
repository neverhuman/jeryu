//! Owner: Docker Control Plane subsystem
//! Proof: `cargo nextest run -p jeryu -- docker`
//! Invariants: Docker calls preserve container ownership labels and surface runtime errors to callers.
//! Docker runtime control for jeryu.
//!
//! Wraps bollard to manage runner-manager containers.

use anyhow::{Context, Result};
use bollard::Docker;

#[path = "docker_manager.rs"]
mod docker_manager;
#[path = "docker_volume.rs"]
mod docker_volume;

pub use docker_manager::RunnerManagerStartSpec;

#[derive(Clone)]
pub struct DockerCtl {
    docker: Docker,
}

impl DockerCtl {
    pub fn connect() -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("connecting to Docker daemon")?;
        Ok(Self { docker })
    }

    /// Create a headless DockerCtl for rendering-only modes (screenshot, capture, once).
    /// Real Docker API calls will fail at request time; only used when demo data is sufficient.
    pub fn disconnected() -> Self {
        // Use a non-routable HTTP endpoint. The client constructs fine; actual Docker
        // API calls would error, which is acceptable for demo-only rendering.
        let docker =
            Docker::connect_with_http("http://127.0.0.1:1", 120, bollard::API_DEFAULT_VERSION)
                .expect("http Docker client");
        Self { docker }
    }
}
