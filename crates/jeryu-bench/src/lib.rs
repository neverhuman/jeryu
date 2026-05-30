//! Benchmark Lab for Jeryu Phase 10.
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

pub use competitors::{all_competitors, all_jeryu_runners};
pub use harness::{BenchmarkHarness, WorkloadProfile, sample_phase10_harness};
pub use models::{CacheState, Competitor, JeryuRunner, ScenarioClass, TrustTier};
pub use receipt::{BenchmarkReceipt, ReceiptError};
pub use replay::{ReplayPlan, ReplayVerdict};
pub use scorecard::{BenchmarkTarget, Scorecard, ScorecardEntry};
