//! Strict-typed loaders for `.jeryu/autonomy/policies/*.yml`.
//!
//! Decision #3: YAML-only policy with named-condition references; no DSL.
//! These loaders accept only canonical policy keys so policy drift fails closed.

use crate::autonomy::freeze::FreezeWindows;
use serde::Deserialize;
use std::path::Path;

#[path = "policy_yaml_types.rs"]
mod types;
pub use types::*;

// --- Bundle loader -------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PolicyBundle {
    pub risk: RiskPolicy,
    pub approvals: ApprovalsPolicy,
    pub release: ReleasePolicy,
    pub protected_paths: ProtectedPathsPolicy,
    /// Strict-typed freeze schedule (vibegate.freeze.v1). `None` when
    /// `.jeryu/autonomy/policies/freeze.yml` is missing — in which case no freeze
    /// enforcement runs, but operators see no error either.
    pub freeze: Option<FreezeWindows>,
}

impl PolicyBundle {
    pub fn from_dir(dir: &Path) -> std::io::Result<Self> {
        let risk: RiskPolicy = read_yaml(&dir.join("risk.yml"))?;
        let approvals: ApprovalsPolicy = read_yaml(&dir.join("approvals.yml"))?;
        let release: ReleasePolicy = read_yaml(&dir.join("release.yml"))?;
        let protected_paths: ProtectedPathsPolicy = read_yaml(&dir.join("protected-paths.yml"))?;
        let freeze_path = dir.join("freeze.yml");
        let freeze: Option<FreezeWindows> = if freeze_path.exists() {
            Some(FreezeWindows::from_path(&freeze_path)?)
        } else {
            None
        };
        Ok(Self {
            risk,
            approvals,
            release,
            protected_paths,
            freeze,
        })
    }
}

fn read_yaml<T: for<'de> Deserialize<'de>>(p: &Path) -> std::io::Result<T> {
    let s = std::fs::read_to_string(p)
        .map_err(|e| std::io::Error::new(e.kind(), format!("read {}: {e}", p.display())))?;
    serde_yaml::from_str(&s).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("parse {}: {e}", p.display()),
        )
    })
}

#[cfg(test)]
#[path = "policy_yaml_tests.rs"]
mod tests;
