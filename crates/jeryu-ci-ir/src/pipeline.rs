//! Top-level pipeline IR: assembly, canonical serialisation, content hashing,
//! and structural validation.

use std::collections::BTreeSet;
use std::fmt;

use crate::enums::PipelineSource;
use crate::enums::TrustTier;
use crate::hashing::{deterministic_hash, line};
use crate::job::{Dependency, Job};
use crate::policy::{
    ArtifactPolicy, CachePolicy, PermissionPolicy, ProofPolicy, SecretPolicy, SigningPolicy,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline {
    pub id: String,
    pub source: PipelineSource,
    pub repo: String,
    pub commit: String,
    pub trust_tier: TrustTier,
    pub jobs: Vec<Job>,
    pub edges: Vec<Dependency>,
    pub jeryu_cache_policy: CachePolicy,
    pub artifact_policy: ArtifactPolicy,
    pub permission_policy: PermissionPolicy,
    pub secret_policy: SecretPolicy,
    pub proof_policy: ProofPolicy,
    pub signing_policy: SigningPolicy,
}

impl Pipeline {
    pub fn new(
        source: PipelineSource,
        repo: impl Into<String>,
        commit: impl Into<String>,
        trust_tier: TrustTier,
    ) -> Self {
        let repo = repo.into();
        let commit = commit.into();
        let id = deterministic_hash(&format!("pipeline|{}|{}|{}", source.as_str(), repo, commit));
        Self {
            id,
            source,
            repo,
            commit,
            trust_tier,
            jobs: Vec::new(),
            edges: Vec::new(),
            jeryu_cache_policy: CachePolicy::default(),
            artifact_policy: ArtifactPolicy::default(),
            permission_policy: PermissionPolicy::default(),
            secret_policy: SecretPolicy::default(),
            proof_policy: ProofPolicy::default(),
            signing_policy: SigningPolicy::default(),
        }
    }

    pub fn canonical(&self) -> String {
        let mut out = String::new();
        line(&mut out, "pipeline.id", &self.id);
        line(&mut out, "pipeline.source", self.source.as_str());
        line(&mut out, "pipeline.repo", &self.repo);
        line(&mut out, "pipeline.commit", &self.commit);
        line(&mut out, "pipeline.trust_tier", self.trust_tier.as_str());
        line(
            &mut out,
            "cache.project_scoped",
            self.jeryu_cache_policy.project_scoped,
        );
        line(
            &mut out,
            "cache.allow_cross_project_compiled",
            self.jeryu_cache_policy.allow_cross_project_compiled,
        );
        line(
            &mut out,
            "cache.promote_after_green",
            self.jeryu_cache_policy.promote_after_green,
        );
        line(
            &mut out,
            "cache.quarantine_untrusted_writes",
            self.jeryu_cache_policy.quarantine_untrusted_writes,
        );
        line(
            &mut out,
            "artifact.allow_absolute_paths",
            self.artifact_policy.allow_absolute_paths,
        );
        line(
            &mut out,
            "artifact.require_metadata",
            self.artifact_policy.require_metadata,
        );
        line(
            &mut out,
            "artifact.default_retention_days",
            self.artifact_policy.default_retention_days,
        );
        line(
            &mut out,
            "permission.fail_closed",
            self.permission_policy.fail_closed,
        );
        line(
            &mut out,
            "secret.secrets_available",
            self.secret_policy.secrets_available,
        );
        line(
            &mut out,
            "secret.deny_on_fork",
            self.secret_policy.deny_on_fork,
        );
        line(&mut out, "proof.required", self.proof_policy.proof_required);
        line(&mut out, "proof.lane", &self.proof_policy.lane);
        line(
            &mut out,
            "signing.provenance_required",
            self.signing_policy.provenance_required,
        );
        line(
            &mut out,
            "signing.release_only",
            self.signing_policy.release_only,
        );

        let mut jobs = self.jobs.clone();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        for job in jobs {
            job.canonical_into(&mut out);
        }

        let mut edges = self.edges.clone();
        edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
        for edge in edges {
            line(&mut out, "edge", format!("{}->{}", edge.from, edge.to));
        }
        out
    }

    pub fn ir_hash(&self) -> String {
        deterministic_hash(&self.canonical())
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.repo.trim().is_empty() {
            return Err(ValidationError::EmptyRepo);
        }
        if self.commit.trim().is_empty() {
            return Err(ValidationError::EmptyCommit);
        }
        let mut seen = BTreeSet::new();
        for job in &self.jobs {
            if job.id.trim().is_empty() {
                return Err(ValidationError::EmptyJobId);
            }
            if !seen.insert(job.id.clone()) {
                return Err(ValidationError::DuplicateJob(job.id.clone()));
            }
            if job.steps.is_empty() {
                return Err(ValidationError::JobHasNoSteps(job.id.clone()));
            }
        }
        for edge in &self.edges {
            if !seen.contains(&edge.from) {
                return Err(ValidationError::UnknownEdgeEndpoint(edge.from.clone()));
            }
            if !seen.contains(&edge.to) {
                return Err(ValidationError::UnknownEdgeEndpoint(edge.to.clone()));
            }
            if edge.from == edge.to {
                return Err(ValidationError::SelfDependency(edge.from.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    EmptyRepo,
    EmptyCommit,
    EmptyJobId,
    DuplicateJob(String),
    JobHasNoSteps(String),
    UnknownEdgeEndpoint(String),
    SelfDependency(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRepo => f.write_str("pipeline repo cannot be empty"),
            Self::EmptyCommit => f.write_str("pipeline commit cannot be empty"),
            Self::EmptyJobId => f.write_str("job id cannot be empty"),
            Self::DuplicateJob(job) => write!(f, "duplicate job id: {job}"),
            Self::JobHasNoSteps(job) => write!(f, "job has no steps: {job}"),
            Self::UnknownEdgeEndpoint(job) => write!(f, "edge references unknown job: {job}"),
            Self::SelfDependency(job) => write!(f, "job cannot depend on itself: {job}"),
        }
    }
}

impl std::error::Error for ValidationError {}
