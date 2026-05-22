use super::{AdmissionEvaluation, AdmissionVerdict};

impl AdmissionVerdict {
    /// Stable string label for persisted admission decisions.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Audit => "audit",
            Self::Deny => "deny",
        }
    }
}

/// Evaluates one standard Git pre-receive line into an admission proof record.
pub fn evaluate_pre_receive_line(line: &str, enforce_agent_grant: bool) -> AdmissionEvaluation {
    evaluate_pre_receive_line_with_context(
        line,
        enforce_agent_grant,
        std::env::var("JERYU_ADMISSION_ACTOR").ok().as_deref(),
        is_non_fast_forward_marker(),
    )
}

#[cfg(test)]
fn evaluate_pre_receive_line_with_actor(
    line: &str,
    enforce_agent_grant: bool,
    actor_override: Option<&str>,
) -> AdmissionEvaluation {
    evaluate_pre_receive_line_with_context(line, enforce_agent_grant, actor_override, false)
}

fn evaluate_pre_receive_line_with_context(
    line: &str,
    enforce_agent_grant: bool,
    actor_override: Option<&str>,
    non_fast_forward: bool,
) -> AdmissionEvaluation {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 3 {
        return AdmissionEvaluation {
            verdict: AdmissionVerdict::Deny,
            old_sha: None,
            new_sha: None,
            ref_name: None,
            actor_kind: "unknown".to_string(),
            reasons: vec!["malformed pre-receive input".to_string()],
            grant_id: None,
            backup_status: None,
            policy_version: "admission-v3.01".to_string(),
        };
    }

    let old_sha = parts[0].to_string();
    let new_sha = parts[1].to_string();
    let ref_name = parts[2].to_string();
    let mut reasons = Vec::new();
    let actor_kind = if actor_override == Some("jeryu") {
        "jeryu"
    } else if is_agent_ref(&ref_name) {
        "agent"
    } else {
        "human_or_system"
    }
    .to_string();

    if !looks_like_git_sha(&old_sha) || !looks_like_git_sha(&new_sha) {
        reasons.push("source or target value is not a git object id".to_string());
    }

    if !ref_name.starts_with("refs/") {
        reasons.push("ref name is not fully qualified".to_string());
    }

    if actor_kind == "agent" {
        reasons.push("agent intent grant verification pending".to_string());
    }
    apply_protected_ref_policy(
        &old_sha,
        &new_sha,
        &ref_name,
        &actor_kind,
        non_fast_forward,
        &mut reasons,
    );

    let invalid_ref = reasons.iter().any(|reason| {
        reason.contains("not a git object id") || reason.contains("not fully qualified")
    });
    let protected_denial = reasons.iter().any(|reason| {
        reason.starts_with("protected branch")
            || reason.starts_with("protected tag")
            || reason.starts_with("non-fast-forward")
    });
    let verdict = if invalid_ref || protected_denial || actor_kind == "agent" && enforce_agent_grant
    {
        AdmissionVerdict::Deny
    } else if actor_kind == "agent" {
        AdmissionVerdict::Audit
    } else {
        AdmissionVerdict::Allow
    };

    AdmissionEvaluation {
        verdict,
        old_sha: Some(old_sha),
        new_sha: Some(new_sha),
        ref_name: Some(ref_name),
        actor_kind,
        reasons,
        grant_id: None,
        backup_status: Some("not_required".to_string()),
        policy_version: "admission-v3.01".to_string(),
    }
}

fn apply_protected_ref_policy(
    old_sha: &str,
    new_sha: &str,
    ref_name: &str,
    actor_kind: &str,
    non_fast_forward: bool,
    reasons: &mut Vec<String>,
) {
    if is_protected_branch(ref_name) {
        if is_delete(new_sha) {
            reasons.push("protected branch removal denied".to_string());
        } else if actor_kind != "jeryu" && !is_create(old_sha) {
            reasons.push("protected branch direct push denied; use JeRyu main-relay".to_string());
        }
        if non_fast_forward {
            reasons.push("non-fast-forward protected branch push denied".to_string());
        }
    }

    if is_protected_tag(ref_name) && !is_create(old_sha) {
        if is_delete(new_sha) {
            reasons.push("protected tag removal denied".to_string());
        } else {
            reasons.push("protected tag rewrite denied".to_string());
        }
    }
}

fn is_protected_branch(ref_name: &str) -> bool {
    matches!(ref_name, "refs/heads/main" | "refs/heads/master")
}

fn is_protected_tag(ref_name: &str) -> bool {
    ref_name.starts_with("refs/tags/v")
}

fn is_create(old_sha: &str) -> bool {
    old_sha.bytes().all(|b| b == b'0')
}

fn is_delete(new_sha: &str) -> bool {
    new_sha.bytes().all(|b| b == b'0')
}

fn is_non_fast_forward_marker() -> bool {
    std::env::var("JERYU_ADMISSION_NON_FAST_FORWARD")
        .ok()
        .as_deref()
        == Some("1")
}

fn is_agent_ref(ref_name: &str) -> bool {
    ref_name.starts_with("refs/heads/agent/")
        || ref_name.starts_with("refs/heads/jeryu/")
        || ref_name.starts_with("refs/heads/agents/")
}

fn looks_like_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod admission_tests;
