//! Benchmark Lab for JitForge Nitro Phase 10.
//!
//! The crate models replayable benchmark receipts and scorecards without relying
//! on external services. Real adapters can execute provider or forge commands,
//! commands, while these core types keep the receipts deterministic and testable.

pub mod competitors;
pub mod harness;
pub mod models;
pub mod receipt;
pub mod replay;
pub mod scorecard;

pub use competitors::{all_competitors, all_jitforge_runners};
pub use harness::{sample_phase10_harness, BenchmarkHarness, WorkloadProfile};
pub use models::{CacheState, Competitor, JitForgeRunner, ScenarioClass, TrustTier};
pub use receipt::{BenchmarkReceipt, ReceiptError};
pub use replay::{ReplayPlan, ReplayVerdict};
pub use scorecard::{BenchmarkTarget, Scorecard, ScorecardEntry};
