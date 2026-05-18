use super::*;

pub(crate) fn build_index(root: &Path) -> Result<AgentIndex> {
    let entries = module_entries(root)?;
    Ok(AgentIndex {
        generated_at: chrono::Utc::now().to_rfc3339(),
        repo_root: root.display().to_string(),
        token_budget_path: "token-budget.toml".to_string(),
        entries,
    })
}

pub(crate) fn module_entries(root: &Path) -> Result<Vec<AgentIndexEntry>> {
    let proof = load_proof_lanes(root)?;
    let files = tracked_source_files(root)?;
    let mut entries = files
        .into_iter()
        .map(|path| build_entry(root, &path, &proof))
        .collect::<Result<Vec<_>>>()?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

pub(crate) fn build_entry(
    root: &Path,
    path: &Path,
    proof: &ProofLanesFile,
) -> Result<AgentIndexEntry> {
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

pub(crate) fn load_proof_lanes(root: &Path) -> Result<ProofLanesFile> {
    let path = root.join("proof-lanes.toml");
    toml::from_str(&fs::read_to_string(&path)?).with_context(|| format!("parse {}", path.display()))
}

pub(crate) fn tracked_source_files(root: &Path) -> Result<Vec<PathBuf>> {
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

pub(crate) fn module_change_type(rel_path: &str, proof: &ProofLanesFile) -> String {
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

pub(crate) fn header_value(content: &str, label: &str) -> Option<String> {
    let prefix = format!("//! {label}:");
    content.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

pub(crate) fn header_value_or_empty(content: &str, label: &str) -> String {
    header_value(content, label).unwrap_or_default()
}

pub(crate) fn proof_lanes_for_change_type(
    proof: &ProofLanesFile,
    default_change_type: &str,
) -> Vec<String> {
    match proof.change_type.get(default_change_type) {
        Some(value) => value.lanes.clone(),
        None => Vec::new(),
    }
}

pub(crate) fn proof_commands_for_lanes(
    proof: &ProofLanesFile,
    proof_lanes: &[String],
) -> Vec<String> {
    let mut commands = Vec::with_capacity(proof_lanes.len());
    for lane in proof_lanes {
        if let Some(value) = proof.lane.get(lane) {
            commands.push(value.command.clone());
        }
    }
    commands
}

pub(crate) fn check_sections(
    path: &Path,
    required: &[&str],
    issues: &mut Vec<AuditIssue>,
) -> Result<bool> {
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

pub(crate) fn render_markdown(index: &AgentIndex) -> String {
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

pub(crate) fn read_text_or_empty(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

pub(crate) fn generated_index_is_current(
    json_path: &Path,
    expected_json: &str,
    markdown_path: &Path,
    expected_markdown: &str,
) -> bool {
    let current_json = read_text_or_empty(json_path);
    let current_markdown = read_text_or_empty(markdown_path);
    compare_generated_index(
        &current_json,
        expected_json,
        &current_markdown,
        expected_markdown,
    )
}

pub(crate) fn compare_generated_index(
    current_json: &str,
    expected_json: &str,
    current_markdown: &str,
    expected_markdown: &str,
) -> bool {
    normalize_index_json(current_json) == normalize_index_json(expected_json)
        && normalize_index_markdown(current_markdown) == normalize_index_markdown(expected_markdown)
}

pub(crate) fn normalize_index_json(raw: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "generated_at".to_string(),
            Value::String("<normalized>".to_string()),
        );
    }
    match serde_json::to_string(&value) {
        Ok(text) => text,
        Err(_) => raw.to_string(),
    }
}

pub(crate) fn normalize_index_markdown(raw: &str) -> String {
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

pub(crate) fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
