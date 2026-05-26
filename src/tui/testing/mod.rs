//! Owner: Interactive TUI subsystem - Flight Deck fixtures
//! Proof: `cargo check -p jeryu --message-format=json`
//! Invariants: Fixtures are deterministic and never read live backends.

mod repo_scenarios;
mod scenarios;

pub use scenarios::{FixtureScenario, ScenarioFixture};

#[cfg(test)]
mod scenarios_tests;
