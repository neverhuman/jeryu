//! Line normalization for the tool-build scanner.
//!
//! The tokenizer is the v1 implementation moved verbatim: identifiers fold to
//! `id`, string/number literals to `lit:str`/`lit:num`, keywords/calls/members/
//! macros keep their (lowercased) names with a role prefix. The per-line
//! product is precomputed once (`joined`, token counts, anchor counts, import
//! flag) so the window pass never re-tokenizes or re-joins.

/// One non-empty normalized source line with everything the window pass needs.
#[derive(Debug, Clone)]
pub(crate) struct NormalizedLine {
    /// 1-based source line number.
    pub line_number: usize,
    /// The normalized tokens joined with single spaces. Window fingerprints
    /// hash these line strings joined with `\n` — byte-identical to v1's
    /// `tokens.join(" ")` per line then `join("\n")` per window.
    pub joined: String,
    /// Token count for the line (`joined.split_whitespace().count()`).
    pub token_count: usize,
    /// Tokens with a `call:`/`macro:`/`member:` role prefix.
    pub anchor_count: usize,
    /// Whether the raw line is an import/use/include style declaration.
    pub is_import: bool,
}

/// Normalize a file's contents into non-empty normalized lines.
pub(crate) fn normalized_lines(contents: &str) -> Vec<NormalizedLine> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let tokens = normalize_line(line);
            if tokens.is_empty() {
                return None;
            }
            let token_count = tokens.len();
            let anchor_count = tokens
                .iter()
                .filter(|token| {
                    token.starts_with("call:")
                        || token.starts_with("macro:")
                        || token.starts_with("member:")
                })
                .count();
            Some(NormalizedLine {
                line_number: idx + 1,
                joined: tokens.join(" "),
                token_count,
                anchor_count,
                is_import: is_import_line(line),
            })
        })
        .collect()
}

/// Whether a raw source line is an import/use/include style declaration.
/// Used by the v2 import-fraction window filter only.
fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("use ")
        || trimmed.starts_with("pub use ")
        || trimmed.starts_with("pub(crate) use ")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("import\t")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("require(")
        || trimmed.starts_with("const ") && trimmed.contains("= require(")
        || trimmed.starts_with("#include")
        || trimmed.starts_with("source ")
        || trimmed.starts_with(". ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("pub mod ")
        || trimmed.starts_with("export * from")
        || trimmed.starts_with("export { ") && trimmed.contains(" from ")
}

fn normalize_line(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
    {
        return Vec::new();
    }
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            tokens.push("lit:str".to_string());
            i += 1;
            while i < chars.len() {
                let current = chars[i];
                let escaped = i > 0 && chars[i - 1] == '\\';
                i += 1;
                if current == ch && !escaped {
                    break;
                }
            }
        } else if ch.is_ascii_digit() {
            tokens.push("lit:num".to_string());
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        } else if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let lower = ident.to_ascii_lowercase();
            let next = next_non_ws(&chars, i);
            let prev = prev_non_ws(&chars, start);
            if is_keyword(&lower) {
                tokens.push(format!("kw:{lower}"));
            } else if next == Some('!') {
                tokens.push(format!("macro:{lower}"));
            } else if next == Some('(') {
                tokens.push(format!("call:{lower}"));
            } else if matches!(prev, Some('.') | Some(':')) {
                tokens.push(format!("member:{lower}"));
            } else {
                tokens.push("id".to_string());
            }
        } else if is_operator(ch) {
            tokens.push(format!("op:{ch}"));
            i += 1;
        } else {
            i += 1;
        }
    }
    tokens
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_operator(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}' | '(' | ')' | '[' | ']' | '?' | '=' | '>' | '<' | '&' | '|'
    )
}

fn next_non_ws(chars: &[char], mut idx: usize) -> Option<char> {
    while idx < chars.len() {
        if !chars[idx].is_whitespace() {
            return Some(chars[idx]);
        }
        idx += 1;
    }
    None
}

fn prev_non_ws(chars: &[char], mut idx: usize) -> Option<char> {
    while idx > 0 {
        idx -= 1;
        if !chars[idx].is_whitespace() {
            return Some(chars[idx]);
        }
    }
    None
}

fn is_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "async"
            | "await"
            | "break"
            | "class"
            | "const"
            | "continue"
            | "else"
            | "enum"
            | "false"
            | "fn"
            | "for"
            | "function"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "mut"
            | "pub"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "type"
            | "use"
            | "where"
            | "while"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_matches_v1_semantics() {
        let tokens = normalize_line("let response = call_remote(input);");
        assert_eq!(
            tokens,
            vec![
                "kw:let",
                "id",
                "op:=",
                "call:call_remote",
                "op:(",
                "id",
                "op:)"
            ]
        );
    }

    #[test]
    fn comments_and_blank_lines_drop() {
        assert!(normalize_line("   ").is_empty());
        assert!(normalize_line("// comment").is_empty());
        assert!(normalize_line("# comment").is_empty());
        assert!(normalize_line("/* block */").is_empty());
    }

    #[test]
    fn string_and_number_literals_fold() {
        let tokens = normalize_line(r#"retry("x\"y", 42_000)"#);
        assert_eq!(
            tokens,
            vec!["call:retry", "op:(", "lit:str", "lit:num", "op:)"]
        );
    }

    #[test]
    fn member_and_macro_roles_keep_names() {
        // A name before `(` is a call even after `.`; member only otherwise.
        let tokens = normalize_line("foo.bar(); qux.len; baz!(x)");
        assert_eq!(
            tokens,
            vec![
                "id",
                "call:bar",
                "op:(",
                "op:)",
                "id",
                "member:len",
                "macro:baz",
                "op:(",
                "id",
                "op:)"
            ]
        );
    }

    #[test]
    fn non_ascii_content_is_preserved_semantically() {
        // Multibyte chars fail every predicate and are dropped, exactly like
        // v1; trailing comments are tokenized (only LEADING comments drop), so
        // the ascii tail of "ünused" still yields an id.
        let tokens = normalize_line("let x = \"héllo\"; // ünused");
        assert_eq!(tokens, vec!["kw:let", "id", "op:=", "lit:str", "id"]);
    }

    #[test]
    fn import_lines_detected_across_languages() {
        assert!(is_import_line("use std::fmt;"));
        assert!(is_import_line("  pub use crate::scan;"));
        assert!(is_import_line("import { useEffect } from 'react';"));
        assert!(is_import_line("from pathlib import Path"));
        assert!(is_import_line("source ops/ci/lib.sh"));
        assert!(!is_import_line("let imports = 3;"));
    }

    #[test]
    fn normalized_lines_carry_counts_and_line_numbers() {
        let lines = normalized_lines("// top\n\nlet a = call_b(c);\nuse std::io;\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].line_number, 3);
        assert_eq!(lines[0].anchor_count, 1);
        assert!(!lines[0].is_import);
        assert!(lines[1].is_import);
        assert_eq!(
            lines[0].token_count,
            lines[0].joined.split_whitespace().count()
        );
    }
}
