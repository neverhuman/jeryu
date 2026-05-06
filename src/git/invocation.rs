//! Owner: Git invocation model
//! Proof: `cargo test -p jeryu -- git_`
//! Invariants: Invocation captures the user intent before any subprocess side effects happen.

use crate::git::classify::{GitCommandClass, GitRisk, classify_argv};
use crate::git::policy::GitMode;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GitInvocation {
    pub request_id: String,
    pub actor: String,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub class: GitCommandClass,
    pub risk: GitRisk,
    pub mode: GitMode,
}

impl GitInvocation {
    pub fn new(cwd: impl Into<PathBuf>, argv: Vec<String>) -> Self {
        let class = classify_argv(&argv);
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            actor: match std::env::var("USER") {
                Ok(value) if !value.trim().is_empty() => value,
                _ => "unknown".into(),
            },
            cwd: cwd.into(),
            argv,
            risk: class.risk(),
            class,
            mode: GitMode::current(),
        }
    }

    pub fn is_push(&self) -> bool {
        matches!(self.argv.first().map(String::as_str), Some("push"))
    }

    pub fn repo_root_hint(&self) -> Option<&Path> {
        Some(self.cwd.as_path())
    }
}
