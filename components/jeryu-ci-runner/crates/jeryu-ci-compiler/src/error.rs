//! Compiler entry-point configuration and error types.

use std::fmt;

use jeryu_ci_ir::{RunnerClass, TrustTier};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CiKind {
    GitHubActions,
    NativeToml,
}

#[derive(Clone, Debug)]
pub struct CompileContext {
    pub repo: String,
    pub commit: String,
    pub trust_tier: TrustTier,
    pub default_runner: RunnerClass,
}

impl CompileContext {
    pub fn new(repo: impl Into<String>, commit: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            commit: commit.into(),
            trust_tier: TrustTier::InternalBranch,
            default_runner: RunnerClass::NativeRustClean,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    EmptyInput,
    MissingJobs,
    MissingSteps(String),
    InvalidLine(String),
    InvalidRunner(String),
    InvalidDependency(String),
    Validation(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("CI input is empty"),
            Self::MissingJobs => f.write_str("CI input did not define any jobs"),
            Self::MissingSteps(job) => write!(f, "CI job did not define executable steps: {job}"),
            Self::InvalidLine(line) => write!(f, "invalid CI line: {line}"),
            Self::InvalidRunner(runner) => write!(f, "invalid runner class: {runner}"),
            Self::InvalidDependency(dep) => write!(f, "invalid dependency: {dep}"),
            Self::Validation(error) => write!(f, "IR validation failed: {error}"),
        }
    }
}

impl std::error::Error for CompileError {}
