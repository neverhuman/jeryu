//! Deterministic CI intermediate representation: pipelines, jobs, governance
//! policies, and the canonical hashing primitives they share.
//!
//! This module is a thin re-export hub. The implementation lives in cohesive
//! submodules grouped by responsibility:
//! - [`hashing`]: deterministic hashing, id sanitisation, canonical helpers.
//! - [`enums`]: enumerated value types (sources, tiers, runner classes, ...).
//! - [`job`]: step/dependency/job IR and per-job canonical serialisation.
//! - [`policy`]: pipeline-level governance policies and their defaults.
//! - [`pipeline`]: the top-level pipeline, hashing, and validation.

mod enums;
mod hashing;
mod job;
mod pipeline;
mod policy;

pub use enums::*;
pub use hashing::{deterministic_hash, sanitize_id, stable_id, trim_quotes};
pub use job::*;
pub use pipeline::*;
pub use policy::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pipeline() -> Pipeline {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/demo",
            "abc123",
            TrustTier::InternalBranch,
        );
        let mut fmt = Job::new("fmt", "fmt", RunnerClass::NativeRustClean);
        fmt.steps
            .push(Step::run("fmt_0", "fmt", "cargo fmt --check"));
        let mut test = Job::new("test", "test", RunnerClass::NativeRustClean);
        test.steps.push(Step::run("test_0", "test", "cargo test"));
        pipeline.jobs.push(test);
        pipeline.jobs.push(fmt);
        pipeline.edges.push(Dependency {
            from: "fmt".to_string(),
            to: "test".to_string(),
        });
        pipeline
    }

    #[test]
    fn hash_is_stable_for_same_pipeline() -> Result<(), Box<dyn std::error::Error>> {
        let a = sample_pipeline();
        let b = sample_pipeline();
        assert_eq!(a.ir_hash(), b.ir_hash());
        assert_eq!(a.canonical(), b.canonical());
        Ok(())
    }

    #[test]
    fn validation_rejects_unknown_edges() {
        let mut pipeline = sample_pipeline();
        pipeline.edges.push(Dependency {
            from: "missing".to_string(),
            to: "fmt".to_string(),
        });
        assert!(matches!(
            pipeline.validate(),
            Err(ValidationError::UnknownEdgeEndpoint(_))
        ));
    }

    #[test]
    fn sanitize_keeps_deterministic_ids() {
        assert_eq!(sanitize_id("Rust stable / Ubuntu"), "rust_stable_ubuntu");
        assert_eq!(sanitize_id("!!!"), "unnamed");
    }
}
