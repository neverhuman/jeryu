//! Assembly of parsed jobs into a validated [`Pipeline`], including dependency
//! edge resolution and deterministic ordering.

use std::collections::{BTreeMap, BTreeSet};

use jeryu_ci_ir::{Dependency, Job, Pipeline, PipelineSource};

use crate::error::{CompileContext, CompileError};

#[derive(Clone, Debug)]
pub(crate) struct PendingJob {
    pub(crate) origin: String,
    pub(crate) job: Job,
    pub(crate) needs: Vec<String>,
}

pub(crate) fn finish_pipeline(
    source: PipelineSource,
    pending: Vec<PendingJob>,
    context: &CompileContext,
) -> Result<Pipeline, CompileError> {
    if pending.is_empty() {
        return Err(CompileError::MissingJobs);
    }
    let mut pipeline = Pipeline::new(
        source,
        &context.repo,
        &context.commit,
        context.trust_tier.clone(),
    );
    let mut origin_to_jobs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in &pending {
        origin_to_jobs
            .entry(item.origin.clone())
            .or_default()
            .push(item.job.id.clone());
    }
    for item in &pending {
        pipeline.jobs.push(item.job.clone());
    }
    let mut edge_set = BTreeSet::new();
    for item in &pending {
        for need in &item.needs {
            let from_jobs = origin_to_jobs.get(need).ok_or_else(|| {
                CompileError::InvalidDependency(format!("{} needs unknown job {need}", item.origin))
            })?;
            for from in from_jobs {
                let edge = (from.clone(), item.job.id.clone());
                if edge_set.insert(edge.clone()) {
                    pipeline.edges.push(Dependency {
                        from: edge.0,
                        to: edge.1,
                    });
                }
            }
        }
    }
    pipeline.jobs.sort_by(|a, b| a.id.cmp(&b.id));
    pipeline
        .edges
        .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    Ok(pipeline)
}
