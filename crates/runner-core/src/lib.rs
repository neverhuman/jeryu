#![doc = "Shared Phase 4 runner-fabric types and policy logic."]

pub mod error;
pub mod fscheck;
pub mod job;
pub mod policy;
pub mod receipt;
pub mod sandbox;
pub mod trust;

pub use error::{RunnerError, RunnerResult};
pub use job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
pub use policy::{select_runner, CacheWritePolicy, PolicyDecision};
pub use receipt::{Receipt, ReceiptStatus};
pub use sandbox::{CgroupLimits, LandlockRule, MountSpec, SandboxPlan, SeccompProfile};
pub use trust::{RunnerClass, TrustTier};
