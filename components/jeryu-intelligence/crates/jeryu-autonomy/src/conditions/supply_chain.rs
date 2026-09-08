//! Supply-chain hard stops: lockfile-only changes, newly declared external code
//! sources, and lockfile diffs without a matching manifest diff (the classic
//! yanked-package backdoor pattern).

use super::HardStop;
use crate::types::{AgentApprovalReceipt, EvidencePack};

const LOCKFILE_BY_MANIFEST: &[(&str, &str)] = &[
    ("Cargo.lock", "Cargo.toml"),
    ("package-lock.json", "package.json"),
    ("yarn.lock", "package.json"),
    ("pnpm-lock.yaml", "package.json"),
    ("Pipfile.lock", "Pipfile"),
    ("poetry.lock", "pyproject.toml"),
    ("go.sum", "go.mod"),
    ("composer.lock", "composer.json"),
    ("Gemfile.lock", "Gemfile"),
];

pub(super) fn cond_lockfile_only_change(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    p.supply_chain.lockfile_only_change.then(|| HardStop {
        name: "lockfile_only_change".into(),
        reason: "lockfile changed without source change (yanked-package backdoor pattern)".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_introduces_new_external_code_source(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    if p.supply_chain.external_code_sources.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "introduces_new_external_code_source".into(),
        reason: format!(
            "{} new external code source(s) declared",
            p.supply_chain.external_code_sources.len()
        ),
        details: serde_json::json!({ "sources": p.supply_chain.external_code_sources }),
    })
}

pub(super) fn cond_lockfile_diff_without_manifest_diff(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    let paths: std::collections::HashSet<&str> =
        p.changed_files.iter().map(|f| f.path.as_str()).collect();
    let mut orphans: Vec<String> = vec![];
    for (lock, manifest) in LOCKFILE_BY_MANIFEST {
        let lock_touched = paths
            .iter()
            .any(|p| *p == *lock || p.ends_with(&format!("/{lock}")));
        if !lock_touched {
            continue;
        }
        let manifest_touched = paths
            .iter()
            .any(|p| *p == *manifest || p.ends_with(&format!("/{manifest}")));
        if !manifest_touched {
            orphans.push(lock.to_string());
        }
    }
    if orphans.is_empty() {
        return None;
    }
    Some(HardStop {
        name: "lockfile_diff_without_manifest_diff".into(),
        reason: format!(
            "lockfile(s) {:?} changed without matching manifest; classic yanked-package backdoor pattern",
            orphans
        ),
        details: serde_json::json!({ "lockfiles": orphans }),
    })
}
