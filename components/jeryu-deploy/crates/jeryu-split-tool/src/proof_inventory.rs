//! Inventory retained proof authority before any old wrapper can be retired.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

// Include implementations and inputs, not only the wrappers that invoke them.
// In particular lib.sh can strengthen a policy's minimum, and hostile tests
// and release verifiers are separate from ordinary Cargo test discovery.
fn source_kind(relative: &str) -> Option<&'static str> {
    if relative.starts_with(".github/workflows/") {
        Some("workflow")
    } else if matches!(relative, "agent/proof-lanes.toml" | "agent/ci-lanes.toml") {
        Some("proof-declaration")
    } else if matches!(
        relative,
        "Justfile" | "scripts/ci-local.sh" | "ops/ci/pr-ci.sh"
    ) {
        Some("entrypoint")
    } else if relative.starts_with("agent/")
        || relative.starts_with("policies/")
        || relative.contains("baseline")
        || relative == "deny.toml"
    {
        Some("threshold-policy")
    } else if [
        "ops/", "scripts/", "tools/", "tests/", ".cargo/", ".config/", "images/", "ux-qa/",
    ]
    .iter()
    .any(|prefix| relative.starts_with(prefix))
        || relative.contains("/tests/")
        || relative.contains("/examples/")
        || relative.ends_with(".sh")
        || relative.ends_with("package.json")
        || relative.contains("playwright.config.")
        || relative.contains("vitest.config.")
        || relative.contains("lighthouserc.")
        || relative.starts_with("crates/jeryu-split-tool/")
        || relative.starts_with("crates/jeryu-tool-control/")
        || relative.starts_with("crates/jeryu-release-toolkit/")
        || relative == "tool-manifest.toml"
    {
        Some("proof-implementation-or-input")
    } else {
        None
    }
}

pub(super) fn run(root: &Path, check: bool) -> Result<()> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()?;
    ensure!(output.status.success(), "cannot enumerate source files");
    let files: BTreeSet<_> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(std::str::from_utf8)
        .collect::<std::result::Result<_, _>>()?;
    let mut inventory = Vec::new();
    for path in files {
        let (component, relative) = match path.strip_prefix("components/") {
            Some(rest) => rest.split_once('/').context("component source path")?,
            None => ("jeryu", path),
        };
        let Some(kind) = source_kind(relative) else {
            continue;
        };
        let absolute = root.join(path);
        ensure!(
            absolute.is_file() && !absolute.is_symlink(),
            "proof source must be a regular file: {path}"
        );
        let digest = Command::new("sha256sum").arg(&absolute).output()?;
        ensure!(digest.status.success(), "cannot hash proof source");
        let digest = String::from_utf8(digest.stdout)?;
        let digest = digest.split_whitespace().next().context("SHA-256")?;
        let declaration: Value = if kind == "proof-declaration" {
            let text = fs::read_to_string(&absolute)?;
            serde_json::to_value(toml::from_str::<toml::Value>(&text)?)?
        } else {
            Value::Null
        };
        inventory.push(json!({
            "component": component, "path": path, "kind": kind, "sha256": digest,
            "declaration": declaration,
            "port_status": "pending-equivalence-proof",
        }));
    }
    ensure!(
        inventory
            .iter()
            .filter(|item| item["kind"] == "workflow")
            .count()
            >= 10,
        "component workflow inventory is incomplete"
    );
    let report = json!({
        "schema_version": "jeryu.proof-inventory/v2",
        "portable_parity_qualified": false,
        "retirement_allowed": false,
        "note": "Implementation/input hashes include root commands, transitive shell tools, hostile tests, policies and release validators. See CI-COVERAGE.md for explicit command mappings and unported gates. Hash agreement is not execution or equivalence evidence; the legacy wrapper does not execute every auxiliary workflow.",
        "sources": inventory,
    });
    let rendered = format!("{}\n", serde_json::to_string_pretty(&report)?);
    if check {
        ensure!(
            fs::read_to_string(root.join("docs/migration/proof-inventory.json"))? == rendered,
            "proof inventory drift; regenerate with jeryu-split proof-inventory"
        );
        println!(
            "retained proof inventory matches source; portable equivalence remains unqualified"
        );
    } else {
        print!("{rendered}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventories_indirect_proof_implementations_and_thresholds() {
        for path in [
            "ops/ci/lib.sh",
            "ops/ci/proof_evidence.sh",
            "ops/ci/coverage-baseline.tsv",
            "tests/sandbox_escape_matrix.sh",
            "scripts/emit-release-receipt.sh",
            "tools/security-lane.sh",
            "crates/jeryu-runner-oci/tests/real_docker_smoke.rs",
            "agent/owner-map.json",
            "agent/coverage-sources.toml",
            "crates/jeryu-tool-control/src/pin.rs",
            "scripts/ci.sh",
        ] {
            assert!(source_kind(path).is_some(), "missing proof source: {path}");
        }
        assert!(source_kind("crates/jeryu-runner-oci/examples/oci_probe.rs").is_some());
        assert_eq!(source_kind("docs/migration/proof-inventory.json"), None);
        assert_eq!(source_kind("target/ci/evidence.json"), None);
    }
}
