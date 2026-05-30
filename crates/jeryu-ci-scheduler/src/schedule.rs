//! Deterministic CI DAG scheduling.

use jeryu_ci_ir::{Pipeline, deterministic_hash};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One parallelizable schedule round.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleRound {
    /// Zero-based round index.
    pub index: usize,
    /// Job ids runnable in this round.
    pub jobs: Vec<String>,
}

/// Deterministic schedule for a pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schedule {
    /// Stable hash over pipeline IR and round layout.
    pub schedule_hash: String,
    /// Parallelizable rounds.
    pub rounds: Vec<ScheduleRound>,
}

/// Scheduling error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleError {
    /// Duplicate job id.
    DuplicateJob(String),
    /// Edge references an unknown job.
    UnknownJob(String),
    /// DAG contains a cycle.
    Cycle,
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateJob(job) => write!(f, "duplicate job id: {job}"),
            Self::UnknownJob(job) => write!(f, "edge references unknown job: {job}"),
            Self::Cycle => f.write_str("pipeline dependency graph contains a cycle"),
        }
    }
}

impl std::error::Error for ScheduleError {}

/// Builds a deterministic topological schedule.
pub fn deterministic_schedule(pipeline: &Pipeline) -> Result<Schedule, ScheduleError> {
    let mut remaining = BTreeSet::new();
    for job in &pipeline.jobs {
        if !remaining.insert(job.id.clone()) {
            return Err(ScheduleError::DuplicateJob(job.id.clone()));
        }
    }

    let mut dependencies_by_job: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in &pipeline.edges {
        if !remaining.contains(&edge.from) {
            return Err(ScheduleError::UnknownJob(edge.from.clone()));
        }
        if !remaining.contains(&edge.to) {
            return Err(ScheduleError::UnknownJob(edge.to.clone()));
        }
        dependencies_by_job
            .entry(edge.to.clone())
            .or_default()
            .insert(edge.from.clone());
    }

    let mut completed = BTreeSet::new();
    let mut rounds = Vec::new();
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|job| {
                dependencies_by_job
                    .get(*job)
                    .is_none_or(|deps| deps.is_subset(&completed))
            })
            .cloned()
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err(ScheduleError::Cycle);
        }
        for job in &ready {
            remaining.remove(job);
            completed.insert(job.clone());
        }
        rounds.push(ScheduleRound {
            index: rounds.len(),
            jobs: ready,
        });
    }

    let mut canonical = String::new();
    canonical.push_str(&pipeline.ir_hash());
    canonical.push('\n');
    for round in &rounds {
        canonical.push_str(&round.index.to_string());
        canonical.push('=');
        canonical.push_str(&round.jobs.join(","));
        canonical.push('\n');
    }

    Ok(Schedule {
        schedule_hash: deterministic_hash(&canonical),
        rounds,
    })
}

#[cfg(test)]
mod tests {
    use super::{ScheduleError, deterministic_schedule};
    use jeryu_ci_ir::{Dependency, Job, Pipeline, PipelineSource, RunnerClass, Step, TrustTier};

    #[test]
    fn schedules_independent_jobs_before_dependents() {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/repo",
            "abc",
            TrustTier::InternalBranch,
        );
        let mut build = Job::new("build", "build", RunnerClass::NativeRustClean);
        build
            .steps
            .push(Step::run("build_0", "build", "cargo build"));
        let mut fmt = Job::new("fmt", "fmt", RunnerClass::NativeRustClean);
        fmt.steps.push(Step::run("fmt_0", "fmt", "cargo fmt"));
        let mut test = Job::new("test", "test", RunnerClass::NativeRustClean);
        test.steps.push(Step::run("test_0", "test", "cargo test"));
        pipeline.jobs = vec![test, build, fmt];
        pipeline.edges = vec![
            Dependency {
                from: "build".to_owned(),
                to: "test".to_owned(),
            },
            Dependency {
                from: "fmt".to_owned(),
                to: "test".to_owned(),
            },
        ];

        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        assert_eq!(schedule.rounds[0].jobs, vec!["build", "fmt"]);
        assert_eq!(schedule.rounds[1].jobs, vec!["test"]);
    }

    #[test]
    fn duplicate_job_ids_fail_before_scheduling() {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/repo",
            "abc",
            TrustTier::InternalBranch,
        );
        pipeline.jobs = vec![
            Job::new("test", "first", RunnerClass::NativeRustClean),
            Job::new("test", "second", RunnerClass::NativeRustClean),
        ];

        let err = deterministic_schedule(&pipeline).unwrap_err();
        assert_eq!(err, ScheduleError::DuplicateJob("test".to_string()));
    }

    #[test]
    fn dependency_cycle_fails_closed() {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/repo",
            "abc",
            TrustTier::InternalBranch,
        );
        pipeline.jobs = vec![
            Job::new("a", "a", RunnerClass::NativeRustClean),
            Job::new("b", "b", RunnerClass::NativeRustClean),
        ];
        pipeline.edges = vec![
            Dependency {
                from: "a".to_owned(),
                to: "b".to_owned(),
            },
            Dependency {
                from: "b".to_owned(),
                to: "a".to_owned(),
            },
        ];

        let err = deterministic_schedule(&pipeline).unwrap_err();
        assert_eq!(err, ScheduleError::Cycle);
    }
}
