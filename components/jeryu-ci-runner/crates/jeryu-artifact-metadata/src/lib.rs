use jeryu_ci_ir::{ArtifactWhen, Pipeline, deterministic_hash};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub pipeline_id: String,
    pub job_id: String,
    pub name: String,
    pub paths: Vec<String>,
    pub when: ArtifactWhen,
    pub retention_days: u32,
    pub digest_hint: String,
    pub provenance_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactError {
    AbsolutePath { job_id: String, path: String },
    ParentTraversal { job_id: String, path: String },
    EmptyPath { job_id: String },
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AbsolutePath { job_id, path } => {
                write!(f, "artifact path for {job_id} must be relative: {path}")
            }
            Self::ParentTraversal { job_id, path } => write!(
                f,
                "artifact path for {job_id} cannot traverse parent dirs: {path}"
            ),
            Self::EmptyPath { job_id } => write!(f, "artifact path for {job_id} cannot be empty"),
        }
    }
}

impl std::error::Error for ArtifactError {}

pub fn plan_artifacts(pipeline: &Pipeline) -> Result<Vec<ArtifactRecord>, ArtifactError> {
    let mut records = Vec::new();
    for job in &pipeline.jobs {
        for artifact in &job.artifact_paths {
            validate_paths(
                &job.id,
                &artifact.paths,
                pipeline.artifact_policy.allow_absolute_paths,
            )?;
            let canonical = format!(
                "artifact|{}|{}|{}|{}|{}|{}",
                pipeline.id,
                job.id,
                artifact.name,
                artifact.when.as_str(),
                artifact.retention_days,
                artifact.paths.join(",")
            );
            records.push(ArtifactRecord {
                id: deterministic_hash(&canonical),
                pipeline_id: pipeline.id.clone(),
                job_id: job.id.clone(),
                name: artifact.name.clone(),
                paths: artifact.paths.clone(),
                when: artifact.when.clone(),
                retention_days: artifact.retention_days,
                digest_hint: deterministic_hash(&format!("digest-hint|{canonical}")),
                provenance_required: pipeline.signing_policy.provenance_required,
            });
        }
    }
    records.sort_by(|a, b| (&a.job_id, &a.name).cmp(&(&b.job_id, &b.name)));
    Ok(records)
}

fn validate_paths(
    job_id: &str,
    paths: &[String],
    allow_absolute: bool,
) -> Result<(), ArtifactError> {
    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(ArtifactError::EmptyPath {
                job_id: job_id.to_string(),
            });
        }
        if !allow_absolute && trimmed.starts_with('/') {
            return Err(ArtifactError::AbsolutePath {
                job_id: job_id.to_string(),
                path: path.clone(),
            });
        }
        if trimmed.split('/').any(|part| part == "..") {
            return Err(ArtifactError::ParentTraversal {
                job_id: job_id.to_string(),
                path: path.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_ci_ir::{
        ArtifactPath, ArtifactWhen, Job, Pipeline, PipelineSource, RunnerClass, Step, TrustTier,
    };

    fn pipeline_with_path(path: &str) -> Pipeline {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/demo",
            "abc123",
            TrustTier::InternalBranch,
        );
        let mut job = Job::new("test", "test", RunnerClass::NativeRustClean);
        job.steps.push(Step::run("s", "test", "cargo test"));
        job.artifact_paths.push(ArtifactPath {
            name: "junit".to_string(),
            paths: vec![path.to_string()],
            when: ArtifactWhen::Always,
            retention_days: 14,
        });
        pipeline.jobs.push(job);
        pipeline
    }

    #[test]
    fn plans_artifact_records() -> Result<(), Box<dyn std::error::Error>> {
        let records = plan_artifacts(&pipeline_with_path("target/junit.xml"))?;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].job_id, "test");
        Ok(())
    }

    #[test]
    fn rejects_absolute_path_by_default() {
        let result = plan_artifacts(&pipeline_with_path("/etc/passwd"));
        assert!(matches!(result, Err(ArtifactError::AbsolutePath { .. })));
    }

    #[test]
    fn rejects_parent_traversal() {
        let result = plan_artifacts(&pipeline_with_path("target/../secret"));
        assert!(matches!(result, Err(ArtifactError::ParentTraversal { .. })));
    }
}
