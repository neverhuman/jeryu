//! Indentation-aware line lexing and the small YAML/list scalar helpers shared
//! by the GitHub Actions and native parsers.

use jeryu_ci_ir::trim_quotes;

#[derive(Clone, Debug)]
pub(crate) struct SourceLine {
    pub(crate) indent: usize,
    pub(crate) text: String,
}

pub(crate) fn collect_lines(input: &str) -> Vec<SourceLine> {
    input
        .lines()
        .filter_map(|raw| {
            let without_comment = strip_comment(raw);
            if without_comment.trim().is_empty() {
                return None;
            }
            let indent = without_comment.chars().take_while(|ch| *ch == ' ').count();
            Some(SourceLine {
                indent,
                text: without_comment.trim().to_string(),
            })
        })
        .collect()
}

pub(crate) fn strip_comment(raw: &str) -> &str {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('#') { "" } else { raw }
}

pub(crate) fn find_line(lines: &[SourceLine], indent: usize, text: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.indent == indent && line.text == text)
}

pub(crate) fn is_yaml_map_header(text: &str) -> bool {
    text.ends_with(':') && !text.starts_with('-') && !text.contains("::")
}

pub(crate) fn header_name(text: &str) -> String {
    jeryu_ci_ir::sanitize_id(text.trim_end_matches(':').trim())
}

pub(crate) fn scalar_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn nested_slice(
    lines: &[SourceLine],
    start: usize,
    parent_indent: usize,
) -> (&[SourceLine], usize) {
    let mut end = start;
    while end < lines.len() && lines[end].indent > parent_indent {
        end += 1;
    }
    (&lines[start..end], end)
}

pub(crate) fn parse_array_or_scalar(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
        return inner
            .split(',')
            .map(|item| trim_quotes(item.trim()).to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    vec![trim_quotes(trimmed).to_string()]
}

pub(crate) fn parse_yaml_list(lines: &[SourceLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| line.text.strip_prefix("- "))
        .map(|item| trim_quotes(item.trim()).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn is_block_scalar(value: &str) -> bool {
    matches!(value.trim(), "|" | "|-" | "|+" | ">" | ">-" | ">+")
}

pub(crate) fn collect_block_scalar(
    lines: &[SourceLine],
    start: usize,
    parent_indent: usize,
    marker: &str,
) -> (String, usize) {
    let mut body = Vec::new();
    let mut end = start;
    while end < lines.len() && lines[end].indent > parent_indent {
        body.push(lines[end].text.clone());
        end += 1;
    }
    if marker.trim().starts_with('>') {
        (body.join(" "), end)
    } else {
        (body.join("\n"), end)
    }
}
