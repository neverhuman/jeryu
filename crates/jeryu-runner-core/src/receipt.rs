#![doc = "Append-only receipt representation for runner dispatch."]

use crate::job::JobRequest;
use crate::policy::PolicyDecision;
use crate::sandbox::SandboxPlan;
use std::time::{SystemTime, UNIX_EPOCH};

/// Receipt status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptStatus {
    /// Planned but not executed.
    Planned,
    /// Job completed successfully.
    Passed,
    /// Job failed.
    Failed,
    /// Job was denied by policy.
    Denied,
}

impl ReceiptStatus {
    /// Stable status label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Denied => "denied",
        }
    }
}

/// Dispatch receipt for every runner decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// Receipt id.
    pub receipt_id: String,
    /// Job id.
    pub job_id: String,
    /// Repository id.
    pub repo_id: String,
    /// Commit SHA.
    pub commit_sha: String,
    /// Runner class.
    pub runner_class: String,
    /// Trust tier.
    pub trust_tier: String,
    /// Status.
    pub status: ReceiptStatus,
    /// Exit code if process executed.
    pub exit_code: Option<i32>,
    /// Started timestamp milliseconds.
    pub started_ms: u128,
    /// Finished timestamp milliseconds.
    pub finished_ms: u128,
    /// Policy reasons.
    pub reasons: Vec<String>,
    /// Sandbox explanation.
    pub sandbox: String,
    /// Additional message.
    pub message: String,
}

impl Receipt {
    /// Create a receipt from request, decision, plan, status and message.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        job: &JobRequest,
        decision: &PolicyDecision,
        plan: &SandboxPlan,
        status: ReceiptStatus,
        exit_code: Option<i32>,
        started_ms: u128,
        finished_ms: u128,
        message: impl Into<String>,
    ) -> Self {
        Self {
            receipt_id: format!(
                "rcpt_{}_{}_{}",
                job.job_id,
                decision.runner_class.as_str().replace('-', "_"),
                finished_ms
            ),
            job_id: job.job_id.clone(),
            repo_id: job.repo_id.clone(),
            commit_sha: job.commit_sha.clone(),
            runner_class: decision.runner_class.as_str().to_string(),
            trust_tier: job.trust_tier.as_str().to_string(),
            status,
            exit_code,
            started_ms,
            finished_ms,
            reasons: decision.reasons.clone(),
            sandbox: plan.explain(),
            message: message.into(),
        }
    }

    /// Create a policy denial receipt without a full decision.
    pub fn denied(job: &JobRequest, message: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            receipt_id: format!("rcpt_{}_denied_{}", job.job_id, now),
            job_id: job.job_id.clone(),
            repo_id: job.repo_id.clone(),
            commit_sha: job.commit_sha.clone(),
            runner_class: "denied".to_string(),
            trust_tier: job.trust_tier.as_str().to_string(),
            status: ReceiptStatus::Denied,
            exit_code: None,
            started_ms: now,
            finished_ms: now,
            reasons: vec!["policy_denied".to_string()],
            sandbox: "none".to_string(),
            message: message.into(),
        }
    }

    /// Serialize receipt to deterministic JSON without external dependencies.
    pub fn to_json(&self) -> String {
        let reasons = self
            .reasons
            .iter()
            .map(|reason| format!("\"{}\"", escape_json(reason)))
            .collect::<Vec<_>>()
            .join(",");
        let exit_code = self
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "null".to_string());
        format!(
            concat!(
                "{{",
                "\"receipt_id\":\"{}\",",
                "\"job_id\":\"{}\",",
                "\"repo_id\":\"{}\",",
                "\"commit_sha\":\"{}\",",
                "\"runner_class\":\"{}\",",
                "\"trust_tier\":\"{}\",",
                "\"status\":\"{}\",",
                "\"exit_code\":{},",
                "\"started_ms\":{},",
                "\"finished_ms\":{},",
                "\"reasons\":[{}],",
                "\"sandbox\":\"{}\",",
                "\"message\":\"{}\"",
                "}}"
            ),
            escape_json(&self.receipt_id),
            escape_json(&self.job_id),
            escape_json(&self.repo_id),
            escape_json(&self.commit_sha),
            escape_json(&self.runner_class),
            escape_json(&self.trust_tier),
            self.status.as_str(),
            exit_code,
            self.started_ms,
            self.finished_ms,
            reasons,
            escape_json(&self.sandbox),
            escape_json(&self.message),
        )
    }
}

/// Current UNIX time in milliseconds.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn escape_json(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json() {
        assert_eq!(escape_json("a\"b\\c"), "a\\\"b\\\\c");
    }
}
