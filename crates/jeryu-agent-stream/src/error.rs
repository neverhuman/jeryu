use serde::{Deserialize, Serialize};

/// Typed repair shape for stream failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStreamRepair {
    /// What operation failed.
    pub purpose: String,
    /// Why it failed.
    pub reason: String,
    /// Operator-actionable fixes.
    pub common_fixes: Vec<String>,
    /// Owning documentation URL.
    pub docs_url: String,
    /// Local rerun or repair hint.
    pub repair_hint: String,
}

/// Stream error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {}", repair.reason)]
pub struct AgentStreamError {
    /// Stable machine code.
    pub code: String,
    /// Required repair fields.
    pub repair: Box<AgentStreamRepair>,
}

impl AgentStreamError {
    /// Construct a typed stream error.
    #[must_use]
    pub fn new(
        code: impl Into<String>,
        purpose: impl Into<String>,
        reason: impl Into<String>,
        common_fixes: &[&str],
        docs_url: impl Into<String>,
        repair_hint: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            repair: Box::new(AgentStreamRepair {
                purpose: purpose.into(),
                reason: reason.into(),
                common_fixes: common_fixes.iter().map(|fix| (*fix).to_string()).collect(),
                docs_url: docs_url.into(),
                repair_hint: repair_hint.into(),
            }),
        }
    }
}
