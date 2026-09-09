use super::*;

pub(super) fn repo_root(
    tool_root: &Path,
    family_root: &Path,
    name: &str,
    overrides: &BTreeMap<String, PathBuf>,
) -> PathBuf {
    overrides.get(name).cloned().unwrap_or_else(|| {
        if name == "jeryu-tool" {
            tool_root.to_owned()
        } else {
            family_root.join(name)
        }
    })
}

pub(super) fn consumer_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let fixed = [
        "ops/ci/ensure-jankurai.sh",
        "ops/ci/lib.sh",
        "ops/ci/common.sh",
        "ops/ci/coverage.sh",
        "ops/ci/pr-ci.sh",
        "ci-fast-push.sh",
        "agent/ci-lanes.toml",
        "agent/audit-policy.toml",
        "agent/native-cli-manifest.toml",
        "images/agent-sandbox/Dockerfile",
        "images/agent-sandbox/README.md",
        "images/agent-sandbox/jankurai-installation-receipt.json",
        "ops/agent-sandbox/smoke.sh",
        "ops/ci/test-governed-jankurai.sh",
        "scripts/ci-doctor.sh",
        "crates/jeryu-api/src/ci_bridge.rs",
        "crates/jeryu-api/tests/jankurai_governance.rs",
        "crates/jeryu-repogate/tests/ci_lanes.rs",
        "docs/release.md",
        "docs/testing.md",
        "docs/audit-rules.md",
        "policy/default-audit-policy.toml",
    ];
    let mut paths: BTreeSet<PathBuf> = fixed
        .iter()
        .map(|relative| root.join(relative))
        .filter(|path| path.is_file())
        .collect();
    let workflows = root.join(".github/workflows");
    if workflows.is_dir() {
        for entry in fs::read_dir(&workflows)
            .map_err(|error| format!("failed to read {}: {error}", workflows.display()))?
        {
            let path = entry
                .map_err(|error| format!("failed to read {}: {error}", workflows.display()))?
                .path();
            if path.extension().and_then(|value| value.to_str()) == Some("yml") {
                paths.insert(path);
            }
        }
    }
    Ok(paths.into_iter().collect())
}
