//! Inventory retained proof authority before any old wrapper can be retired.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path, process::Command};

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
        let Some(rest) = path.strip_prefix("components/") else {
            continue;
        };
        let Some((component, relative)) = rest.split_once('/') else {
            continue;
        };
        let kind = if relative.starts_with(".github/workflows/") {
            "workflow"
        } else if matches!(relative, "agent/proof-lanes.toml" | "agent/ci-lanes.toml") {
            "proof-declaration"
        } else if matches!(
            relative,
            "Justfile" | "scripts/ci-local.sh" | "ops/ci/pr-ci.sh"
        ) {
            "entrypoint"
        } else if matches!(
            relative,
            "agent/audit-policy.toml"
                | "agent/coverage-sources.toml"
                | "ops/ci/coverage-baseline.tsv"
        ) {
            "threshold-policy"
        } else {
            continue;
        };
        let absolute = root.join(path);
        if !absolute.is_file() {
            continue;
        }
        ensure!(
            !absolute.is_symlink(),
            "proof source must be a regular file"
        );
        let text = fs::read_to_string(&absolute)?;
        let digest = Command::new("sha256sum").arg(&absolute).output()?;
        ensure!(digest.status.success(), "cannot hash proof source");
        let digest = String::from_utf8(digest.stdout)?;
        let digest = digest.split_whitespace().next().context("SHA-256")?;
        let declaration: Value = if kind == "proof-declaration"
            || (kind == "threshold-policy" && relative.ends_with(".toml"))
        {
            serde_json::to_value(toml::from_str::<toml::Value>(&text)?)?
        } else {
            Value::String(text)
        };
        inventory.push(json!({
            "component": component, "path": path, "kind": kind, "sha256": digest,
            "declaration": declaration,
            "shared_root_command": "scripts/ci.sh legacy",
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
        "schema_version": "jeryu.proof-inventory/v1",
        "portable_parity_qualified": false,
        "retirement_allowed": false,
        "note": "The legacy lane preserves existing entrypoints. Every declaration and threshold below remains required until a portable replacement has an exact-source equivalence proof; this inventory does not claim that the legacy wrapper executes every auxiliary workflow.",
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
