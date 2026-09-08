//! Private enforcement classification and focused tests.

use super::EnforcementLevel;
#[cfg(test)]
use super::EnforcementReport;

pub(super) fn classify(level: &EnforcementLevel) -> (Vec<String>, Vec<String>) {
    let all = [
        "no_new_privs",
        "cgroup_v2",
        "landlock",
        "seccomp",
        "user_namespace",
        "mount_namespace",
        "pid_namespace",
    ];
    match level {
        EnforcementLevel::Enforced => (all.iter().map(|s| (*s).to_string()).collect(), Vec::new()),
        EnforcementLevel::Unavailable { .. } => {
            (Vec::new(), all.iter().map(|s| (*s).to_string()).collect())
        }
        EnforcementLevel::Degraded { missing } => {
            let applied = all
                .iter()
                .filter(|s| !missing.contains(&(**s).to_string()))
                .map(|s| (*s).to_string())
                .collect();
            (applied, missing.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforcement_report_json_is_stable() {
        let report = EnforcementReport {
            level: "degraded".to_string(),
            applied: vec!["no_new_privs".to_string(), "seccomp".to_string()],
            skipped: vec!["user_namespace".to_string()],
            proc_no_new_privs: Some(1),
            proc_seccomp: Some(2),
        };
        let json = report.to_json();
        assert!(json.contains("\"level\":\"degraded\""));
        assert!(json.contains("\"proc_no_new_privs\":1"));
        assert!(json.contains("\"proc_seccomp\":2"));
        assert!(json.contains("\"user_namespace\""));
    }

    #[test]
    fn classify_enforced_marks_all_applied() {
        let (applied, skipped) = classify(&EnforcementLevel::Enforced);
        assert!(skipped.is_empty());
        assert!(applied.contains(&"seccomp".to_string()));
        assert!(applied.contains(&"landlock".to_string()));
    }

    #[test]
    fn classify_degraded_splits_missing() {
        let level = EnforcementLevel::Degraded {
            missing: vec!["user_namespace".to_string(), "mount_namespace".to_string()],
        };
        let (applied, skipped) = classify(&level);
        assert!(skipped.contains(&"user_namespace".to_string()));
        assert!(applied.contains(&"seccomp".to_string()));
        assert!(!applied.contains(&"user_namespace".to_string()));
    }
}
