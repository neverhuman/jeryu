//! Owner: Agent Surface
//! Proof: `cargo check -p jeryu && cargo test -p jeryu agent_surface`
//! Invariants: Generated routing index is derived from repo truth; audit fails on missing hard surfaces and outdated generated output.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED_ROOT_SECTIONS: &[&str] = &[
    "Proof Routing",
    "Proof Commands",
    "Module Ownership",
    "Cross-Repo Contract",
    "Guardrails",
    "Diagnostics",
    "Token Optimization",
];

#[derive(Debug, Clone, Deserialize)]
struct ProofLanesFile {
    #[serde(default)]
    lane: BTreeMap<String, ProofLane>,
    #[serde(default)]
    change_type: BTreeMap<String, ChangeType>,
    #[serde(default)]
    module_hints: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProofLane {
    command: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChangeType {
    #[serde(default)]
    lanes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentIndex {
    generated_at: String,
    repo_root: String,
    token_budget_path: String,
    entries: Vec<AgentIndexEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentIndexEntry {
    id: String,
    path: String,
    owner: String,
    proof: String,
    invariants: String,
    default_change_type: String,
    proof_lanes: Vec<String>,
    proof_commands: Vec<String>,
    widening_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AgentSurfaceAudit {
    ok: bool,
    token_budget_present: bool,
    root_agents_ok: bool,
    rtk_doc_present: bool,
    index_current: bool,
    modules_checked: usize,
    issues: Vec<AuditIssue>,
    warnings: Vec<AuditIssue>,
}

#[derive(Debug, Clone, Serialize)]
struct AuditIssue {
    scope: String,
    path: String,
    detail: String,
}

pub fn render_agent_index(check: bool) -> Result<()> {
    let root = repo_root()?;
    let index = build_index(&root)?;
    let json_text = serde_json::to_string_pretty(&index)?;
    let markdown_text = render_markdown(&index);
    let json_path = root.join("agent-index.json");
    let markdown_path = root.join("agent-index.md");

    if check {
        if !generated_index_is_current(&json_path, &json_text, &markdown_path, &markdown_text) {
            bail!(
                "agent index drift detected; run `cargo run -p jeryu -- repo render-agent-index`"
            );
        }
        return Ok(());
    }

    fs::write(&json_path, json_text).with_context(|| format!("write {}", json_path.display()))?;
    fs::write(&markdown_path, markdown_text)
        .with_context(|| format!("write {}", markdown_path.display()))?;
    println!("{}", json_path.display());
    println!("{}", markdown_path.display());
    Ok(())
}

pub fn audit_agent_surface(as_json: bool) -> Result<()> {
    let root = repo_root()?;
    let report = build_audit_report(&root)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Agent surface audit");
        println!(
            "  token budget: {}",
            if report.token_budget_present {
                "ok"
            } else {
                "missing"
            }
        );
        println!(
            "  root AGENTS:  {}",
            if report.root_agents_ok {
                "ok"
            } else {
                "needs work"
            }
        );
        println!(
            "  RTK docs:     {}",
            if report.rtk_doc_present {
                "ok"
            } else {
                "missing"
            }
        );
        println!(
            "  index fresh:  {}",
            if report.index_current {
                "ok"
            } else {
                "outdated"
            }
        );
        println!("  modules:      {}", report.modules_checked);
        if !report.issues.is_empty() {
            println!("\nIssues:");
            for issue in &report.issues {
                println!("  - {} [{}]: {}", issue.scope, issue.path, issue.detail);
            }
        }
        if !report.warnings.is_empty() {
            println!("\nWarnings:");
            for warning in &report.warnings {
                println!(
                    "  - {} [{}]: {}",
                    warning.scope, warning.path, warning.detail
                );
            }
        }
    }
    if !report.ok {
        bail!("agent surface audit failed");
    }
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("current dir")?;
    Ok(cwd)
}

fn build_audit_report(root: &Path) -> Result<AgentSurfaceAudit> {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let token_budget_present = root.join("token-budget.toml").is_file();
    if !token_budget_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "token-budget.toml".to_string(),
            detail: "missing token budget configuration".to_string(),
        });
    }

    let root_agents = root.join("AGENTS.md");
    let root_agents_ok = check_sections(&root_agents, REQUIRED_ROOT_SECTIONS, &mut issues)?;

    let rtk_doc = root.join("docs/RTK.md");
    let rtk_doc_present = rtk_doc.is_file();
    if !rtk_doc_present {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "docs/RTK.md".to_string(),
            detail: "missing RTK usage guidance".to_string(),
        });
    }

    let entries = module_entries(root)?;
    for entry in &entries {
        if entry.owner.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Owner:` header".to_string(),
            });
        }
        if entry.proof.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Proof:` header".to_string(),
            });
        }
        if entry.invariants.trim().is_empty() {
            warnings.push(AuditIssue {
                scope: "module".to_string(),
                path: entry.path.clone(),
                detail: "missing `//! Invariants:` header".to_string(),
            });
        }
    }

    let index = build_index(root)?;
    let expected_json = serde_json::to_string_pretty(&index)?;
    let expected_markdown = render_markdown(&index);
    let index_current = generated_index_is_current(
        &root.join("agent-index.json"),
        &expected_json,
        &root.join("agent-index.md"),
        &expected_markdown,
    );
    if !index_current {
        issues.push(AuditIssue {
            scope: "root".to_string(),
            path: "agent-index.{json,md}".to_string(),
            detail: "generated index is missing or outdated".to_string(),
        });
    }

    Ok(AgentSurfaceAudit {
        ok: issues.is_empty(),
        token_budget_present,
        root_agents_ok,
        rtk_doc_present,
        index_current,
        modules_checked: entries.len(),
        issues,
        warnings,
    })
}

fn build_index(root: &Path) -> Result<AgentIndex> {
    let entries = module_entries(root)?;
    Ok(AgentIndex {
        generated_at: chrono::Utc::now().to_rfc3339(),
        repo_root: root.display().to_string(),
        token_budget_path: "token-budget.toml".to_string(),
        entries,
    })
}

fn module_entries(root: &Path) -> Result<Vec<AgentIndexEntry>> {
    let proof = load_proof_lanes(root)?;
    let files = tracked_source_files(root)?;
    let mut entries = files
        .into_iter()
        .map(|path| build_entry(root, &path, &proof))
        .collect::<Result<Vec<_>>>()?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn build_entry(root: &Path, path: &Path, proof: &ProofLanesFile) -> Result<AgentIndexEntry> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let rel_path = rel(root, path);
    let default_change_type = module_change_type(&rel_path, proof);
    let proof_lanes = proof_lanes_for_change_type(proof, &default_change_type);
    let proof_commands = proof_commands_for_lanes(proof, &proof_lanes);
    let mut widening_rules = Vec::new();
    if default_change_type == "security-relevant" {
        widening_rules.push("security-relevant modules widen to security proof lanes".to_string());
    }
    if default_change_type == "cross-module" || default_change_type == "release-blocking" {
        widening_rules.push("cross-module changes widen to the full integration ring".to_string());
    }
    if rel_path.starts_with("src/test_intel/") || rel_path == "src/impact.rs" {
        widening_rules
            .push("test intelligence changes widen to planner and release validation".to_string());
    }

    Ok(AgentIndexEntry {
        id: rel_path
            .trim_start_matches("src/")
            .trim_end_matches(".rs")
            .replace('/', "::"),
        path: rel_path,
        owner: header_value_or_empty(&content, "Owner"),
        proof: header_value_or_empty(&content, "Proof"),
        invariants: header_value_or_empty(&content, "Invariants"),
        default_change_type,
        proof_lanes,
        proof_commands,
        widening_rules,
    })
}

fn load_proof_lanes(root: &Path) -> Result<ProofLanesFile> {
    let path = root.join("proof-lanes.toml");
    toml::from_str(&fs::read_to_string(&path)?).with_context(|| format!("parse {}", path.display()))
}

fn tracked_source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "src"])
        .output()
        .context("run git ls-files src")?;
    if !output.status.success() {
        bail!("git ls-files src failed");
    }
    let stdout = String::from_utf8(output.stdout).context("decode git ls-files output")?;
    Ok(stdout
        .lines()
        .filter(|line| line.ends_with(".rs"))
        .map(|line| root.join(line))
        .filter(|path| path.exists())
        .collect())
}

fn module_change_type(rel_path: &str, proof: &ProofLanesFile) -> String {
    match proof.module_hints.iter().find_map(|(hint, change_type)| {
        if let Some(prefix) = hint.strip_suffix('/') {
            rel_path
                .starts_with(&format!("src/{prefix}"))
                .then(|| change_type.clone())
        } else {
            rel_path.ends_with(hint).then(|| change_type.clone())
        }
    }) {
        Some(change_type) => change_type,
        None => "leaf-bugfix".to_string(),
    }
}

fn header_value(content: &str, label: &str) -> Option<String> {
    let prefix = format!("//! {label}:");
    content.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

fn header_value_or_empty(content: &str, label: &str) -> String {
    match header_value(content, label) {
        Some(value) => value,
        None => String::new(),
    }
}

fn proof_lanes_for_change_type(proof: &ProofLanesFile, default_change_type: &str) -> Vec<String> {
    match proof.change_type.get(default_change_type) {
        Some(value) => value.lanes.clone(),
        None => Vec::new(),
    }
}

fn proof_commands_for_lanes(proof: &ProofLanesFile, proof_lanes: &[String]) -> Vec<String> {
    let mut commands = Vec::with_capacity(proof_lanes.len());
    for lane in proof_lanes {
        if let Some(value) = proof.lane.get(lane) {
            commands.push(value.command.clone());
        }
    }
    commands
}

fn check_sections(path: &Path, required: &[&str], issues: &mut Vec<AuditIssue>) -> Result<bool> {
    let body = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut ok = true;
    for section in required {
        if !body.contains(&format!("## {section}")) {
            ok = false;
            issues.push(AuditIssue {
                scope: "docs".to_string(),
                path: path.display().to_string(),
                detail: format!("missing section `{section}`"),
            });
        }
    }
    Ok(ok)
}

fn render_markdown(index: &AgentIndex) -> String {
    let mut out = String::new();
    out.push_str("# Agent Index\n\n");
    out.push_str(&format!(
        "Generated: `{}`\n\n| Module | Change Type | Proof Commands | Owner |\n|---|---|---|---|\n",
        index.generated_at
    ));
    for entry in &index.entries {
        let proof = if entry.proof_commands.is_empty() {
            "-".to_string()
        } else {
            entry.proof_commands.join("<br>")
        };
        let owner = if entry.owner.is_empty() {
            "-".to_string()
        } else {
            entry.owner.clone()
        };
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} |\n",
            entry.path, entry.default_change_type, proof, owner
        ));
    }
    out
}

fn compare_generated_index(
    current_json: &str,
    expected_json: &str,
    current_markdown: &str,
    expected_markdown: &str,
) -> bool {
    normalize_index_json(current_json) == normalize_index_json(expected_json)
        && normalize_index_markdown(current_markdown) == normalize_index_markdown(expected_markdown)
}

fn normalize_index_json(raw: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "generated_at".to_string(),
            Value::String("<normalized>".to_string()),
        );
    }
    match serde_json::to_string(&value) { Ok(s) => s, Err(_) => raw.to_string() }
}

fn normalize_index_markdown(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            if line.starts_with("Generated: `") {
                "Generated: `<normalized>`".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_value_extracts_doc_headers() {
        let body = "//! Owner: Agent Surface\n//! Proof: cargo check\n//! Invariants: keep routing derivable\n";
        assert_eq!(
            header_value(body, "Owner").as_deref(),
            Some("Agent Surface")
        );
        assert_eq!(header_value(body, "Proof").as_deref(), Some("cargo check"));
    }

    #[test]
    fn module_change_type_honors_hints() {
        let proof = ProofLanesFile {
            lane: BTreeMap::new(),
            change_type: BTreeMap::new(),
            module_hints: BTreeMap::from([
                ("test_intel/".to_string(), "api-change".to_string()),
                ("secrets.rs".to_string(), "security-relevant".to_string()),
            ]),
        };
        assert_eq!(
            module_change_type("src/test_intel/mod.rs", &proof),
            "api-change"
        );
        assert_eq!(
            module_change_type("src/secrets.rs", &proof),
            "security-relevant"
        );
    }
}
