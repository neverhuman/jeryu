//! Owner: messaging::topics — canonical topic + partition names
//! Proof: `cargo check -p jeryu`
//! Invariants: topic names are stable; renames are breaking changes for consumer state.

pub const JOBS: &str = "jeryu.webhook.jobs";
pub const PIPELINES: &str = "jeryu.webhook.pipelines";
pub const PUSHES: &str = "jeryu.webhook.pushes";

pub const PARTITION_DEFAULT: i32 = 0;

pub const ALL: &[&str] = &[JOBS, PIPELINES, PUSHES];
