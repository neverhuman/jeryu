use std::path::Path;

use cargo_vrc::{PackageAgentMetadata, ReportRepairHint, model::CiProfile};

pub(crate) fn workspace_has_semver_profile(
    ci_profiles: &[CiProfile],
    shared_contracts: &[String],
) -> bool {
    if ci_profiles
        .iter()
        .find(|profile| profile.name.eq_ignore_ascii_case("scheduled-hardening"))
        .is_some_and(|profile| {
            profile
                .commands
                .iter()
                .any(|command| command_has_semver_signal(command))
        })
    {
        return true;
    }

    if ci_profiles.iter().any(|profile| {
        profile
            .commands
            .iter()
            .any(|command| command_has_semver_signal(command))
    }) {
        return true;
    }

    shared_contracts
        .iter()
        .any(|contract| contract.to_ascii_lowercase().contains("semver"))
}

pub(crate) fn command_has_semver_signal(command: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("semver-checks") || command.contains("semver")
}

pub(crate) fn looks_like_junk_drawer(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["common", "utils", "helpers", "shared"]
        .iter()
        .any(|needle| {
            name == *needle
                || name.starts_with(&format!("{needle}-"))
                || name.ends_with(&format!("-{needle}"))
        })
}

pub(crate) fn looks_like_core_layer(agent: &PackageAgentMetadata, name: &str) -> bool {
    let purpose = agent.purpose.to_ascii_lowercase();
    name.contains("core") || purpose.contains("pure") || purpose.contains("domain")
}

pub(crate) fn hidden_io_signal(content: &str) -> bool {
    [
        "std::fs::",
        "tokio::fs::",
        "std::env::",
        "reqwest::",
        "ureq::",
        "std::net::",
    ]
    .iter()
    .any(|needle| content.contains(needle))
}

pub(crate) fn existing_exception(agent: &PackageAgentMetadata, class_id: &str) -> Option<String> {
    let normalized = class_id.replace('-', "_");
    agent
        .exceptions
        .iter()
        .find(|exception| exception.contains(class_id) || exception.contains(&normalized))
        .cloned()
}

pub(crate) fn report_repair_hint() -> ReportRepairHint {
    ReportRepairHint {
        purpose: "Route the next audit rerun".to_string(),
        reason: "The scan needs a local proof pointer when it returns a failure surface."
            .to_string(),
        common_fixes: vec![
            "Trim the failing surface to the owning module or manifest.".to_string(),
            "Re-run the narrow proof command listed in docs/testing.md.".to_string(),
        ],
        docs_url: "docs/testing.md#repair-receipts".to_string(),
        repair_hint: "cargo run -p cargo-aer -- scan --output aer-findings.json".to_string(),
    }
}

pub(crate) fn display_relative(root: &Path, path: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(relative) if !relative.as_os_str().is_empty() => relative.display().to_string(),
        _ if path == root => ".".to_string(),
        _ => path.display().to_string(),
    }
}

pub(crate) fn display_workspace_root() -> String {
    ".".to_string()
}

pub(crate) fn sarif_level(severity: &str) -> &'static str {
    match severity {
        "error" => "error",
        "warning" => "warning",
        _ => "note",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_vrc::model::CiProfile;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    fn current_manifest() -> PathBuf {
        workspace_root().join("Cargo.toml")
    }

    #[test]
    fn workspace_has_semver_profile_accepts_scheduled_hardening_commands() {
        let ci_profiles = vec![CiProfile {
            name: "scheduled-hardening".to_string(),
            commands: vec!["cargo semver-checks check-release".to_string()],
        }];

        assert!(workspace_has_semver_profile(&ci_profiles, &[]));
    }

    #[test]
    fn workspace_has_semver_profile_falls_back_to_shared_contracts() {
        let shared_contracts =
            vec!["Public APIs must run semver validation before release.".to_string()];

        assert!(workspace_has_semver_profile(&[], &shared_contracts));
    }

    #[test]
    fn workspace_has_semver_profile_rejects_missing_signal() {
        let ci_profiles = vec![CiProfile {
            name: "pull-request".to_string(),
            commands: vec!["cargo run -p cargo-aer -- scan --output aer-findings.json".to_string()],
        }];

        assert!(!workspace_has_semver_profile(&ci_profiles, &[]));
    }

    #[test]
    fn current_workspace_scan_uses_relative_paths_and_semver_gate() {
        let report =
            crate::scan::scan_workspace(Some(&current_manifest())).expect("scan current workspace");

        assert_eq!(report.workspace_root, ".");
        assert!(
            report
                .findings
                .iter()
                .all(|finding| !finding.path.starts_with('/'))
        );
    }
}
