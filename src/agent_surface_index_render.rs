use super::*;

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
    match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => String::new(),
    }
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
