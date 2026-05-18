//! Robust LLM-output JSON extraction.
//!
//! Many providers wrap their JSON in code fences, prepend "Here is the JSON:",
//! or emit a chatty preamble. We extract the first top-level `{ ... }` block
//! that successfully deserializes into our expected shape.

use serde::Deserialize;

/// Minimal projection of an `AgentApprovalReceipt`. The full struct demands
/// fields like `id` and `signature` that the LLM cannot mint; the orchestrator
/// fills those in after parsing this projection.
#[derive(Debug, Clone, Deserialize)]
pub struct ParsedReceiptFields {
    pub role: String,
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub findings: Vec<serde_json::Value>,
}

/// Extract the first valid `{ ... }` block from raw model output.
pub fn extract_receipt_json(raw: &str) -> Result<ParsedReceiptFields, String> {
    let cleaned = strip_code_fence(raw);
    // Fast path: cleaned input is already valid.
    if let Ok(v) = serde_json::from_str::<ParsedReceiptFields>(cleaned) {
        return Ok(v);
    }
    // Slow path: scan for balanced top-level `{...}` substrings.
    for candidate in find_top_level_object_substrings(cleaned) {
        if let Ok(v) = serde_json::from_str::<ParsedReceiptFields>(&candidate) {
            return Ok(v);
        }
    }
    Err(format!(
        "no valid receipt JSON in response (first 200 chars: {:?})",
        cleaned.chars().take(200).collect::<String>()
    ))
}

fn strip_code_fence(s: &str) -> &str {
    let trimmed = s.trim();
    // Try the longer fence first so its inner JSON is exposed verbatim;
    // this is a parser tokenizer, not graceful degradation.
    let stripped = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest
    } else {
        trimmed
    };
    stripped.trim_end_matches("```").trim()
}

fn find_top_level_object_substrings(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            // Walk forward counting braces, respecting string literals.
            let mut depth = 0i32;
            let mut in_string = false;
            let mut escape = false;
            let start = i;
            while i < bytes.len() {
                let c = bytes[i];
                if in_string {
                    if escape {
                        escape = false;
                    } else if c == b'\\' {
                        escape = true;
                    } else if c == b'"' {
                        in_string = false;
                    }
                } else {
                    match c {
                        b'"' => in_string = true,
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                out.push(s[start..=i].to_string());
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json() {
        let raw = r#"{"role":"security","decision":"pass"}"#;
        let p = extract_receipt_json(raw).unwrap();
        assert_eq!(p.role, "security");
        assert_eq!(p.decision, "pass");
    }

    #[test]
    fn parses_fenced_json() {
        let raw = "```json\n{\"role\":\"security\",\"decision\":\"block\",\"reason\":\"x\"}\n```";
        let p = extract_receipt_json(raw).unwrap();
        assert_eq!(p.decision, "block");
    }

    #[test]
    fn parses_with_chatty_preamble() {
        let raw = "Here is the receipt:\n\n{\"role\":\"security\",\"decision\":\"concern\"}";
        let p = extract_receipt_json(raw).unwrap();
        assert_eq!(p.decision, "concern");
    }

    #[test]
    fn parses_with_braces_in_strings() {
        let raw = r#"{"role":"security","decision":"block","reason":"format!(\"x={}\", y)"}"#;
        let p = extract_receipt_json(raw).unwrap();
        assert_eq!(p.decision, "block");
        assert!(p.reason.unwrap().contains("format!"));
    }

    #[test]
    fn rejects_invalid_response() {
        let raw = "I cannot comply with this request.";
        let err = extract_receipt_json(raw).unwrap_err();
        assert!(err.contains("no valid receipt JSON"));
    }

    #[test]
    fn parses_findings_array() {
        let raw = r#"{
          "role":"security",
          "decision":"block",
          "findings":[
            {"severity":"critical","class":"injection-sql","file":"a.rs","range":[1,2]}
          ]
        }"#;
        let p = extract_receipt_json(raw).unwrap();
        assert_eq!(p.findings.len(), 1);
    }
}
