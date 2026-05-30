//! Tiny JSON helpers used to avoid product-truth dependencies in Phase 8.

/// Escape a string for JSON output.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render a JSON string field.
pub fn field(name: &str, value: &str) -> String {
    format!("\"{}\":\"{}\"", escape(name), escape(value))
}

/// Render a JSON numeric field.
pub fn number_field(name: &str, value: u64) -> String {
    format!("\"{}\":{}", escape(name), value)
}

/// Render a JSON boolean field.
pub fn bool_field(name: &str, value: bool) -> String {
    format!(
        "\"{}\":{}",
        escape(name),
        if value { "true" } else { "false" }
    )
}
