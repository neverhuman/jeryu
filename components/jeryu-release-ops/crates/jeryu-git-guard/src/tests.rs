use super::*;

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

fn decide(parts: &[&str]) -> GitDecision {
    git_command_decision(&argv(parts), "agents/a/wc/feature")
}

fn reason(decision: &GitDecision) -> &str {
    match decision {
        GitDecision::Deny(error) => error.reason,
        GitDecision::Allow => "ALLOW",
    }
}

mod allowed;
mod denied;
