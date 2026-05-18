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

pub(super) fn string_schema_optional() -> Value {
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

pub(super) fn parse_modifications(
    value: &Value,
) -> Option<Vec<crate::capability::FileModification>> {
    let items = value.as_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let file_path = item.get("file_path")?.as_str()?.to_string();
        let content = item.get("content")?.as_str()?.to_string();
        out.push(crate::capability::FileModification { file_path, content });
    }
    Some(out)
}

pub(super) fn parse_hypotheses(value: &Value) -> Option<Vec<crate::capability::HypothesisPatch>> {
    let items = value.as_array()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let branch_suffix = item.get("branch_suffix")?.as_str()?.to_string();
        let modifications = parse_modifications(item.get("modifications")?)?;
        out.push(crate::capability::HypothesisPatch {
            branch_suffix,
            modifications,
        });
    }
    Some(out)
}
