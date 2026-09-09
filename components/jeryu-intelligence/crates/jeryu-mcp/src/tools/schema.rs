//! JSON Schema construction helpers (near-verbatim port of `tools_schema.rs`).

use serde_json::Value;

pub(super) fn tool_annotations(
    read_only: bool,
    destructive: bool,
    idempotent: bool,
    open_world: bool,
) -> Value {
    serde_json::json!({
        "readOnlyHint": read_only,
        "destructiveHint": destructive,
        "idempotentHint": idempotent,
        "openWorldHint": open_world,
    })
}

pub(super) fn object_schema(required: &[&str], props: &[(&str, Value)]) -> Value {
    let properties = props
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

pub(super) fn string_schema() -> Value {
    serde_json::json!({ "type": "string" })
}

pub(super) fn integer_schema() -> Value {
    serde_json::json!({ "type": "integer" })
}

pub(super) fn array_schema(items: Value) -> Value {
    serde_json::json!({ "type": "array", "items": items })
}

pub(super) fn enum_schema(values: &[&str]) -> Value {
    serde_json::json!({ "type": "string", "enum": values })
}

pub(super) fn parse_string_array(value: &Value) -> Option<Vec<String>> {
    let items = value.as_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(item.as_str()?.to_string());
    }
    Some(out)
}

/// One file modification: `{ file_path, content }`. Validated, then passed through as JSON.
pub(super) fn parse_modifications(value: &Value) -> Option<Vec<Value>> {
    let items = value.as_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let file_path = item.get("file_path")?.as_str()?.to_string();
        let content = item.get("content")?.as_str()?.to_string();
        out.push(serde_json::json!({ "file_path": file_path, "content": content }));
    }
    Some(out)
}

/// One race hypothesis: `{ branch_suffix, modifications[] }`.
pub(super) fn parse_hypotheses(value: &Value) -> Option<Vec<Value>> {
    let items = value.as_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let branch_suffix = item.get("branch_suffix")?.as_str()?.to_string();
        let modifications = parse_modifications(item.get("modifications")?)?;
        out.push(serde_json::json!({
            "branch_suffix": branch_suffix,
            "modifications": modifications,
        }));
    }
    Some(out)
}
