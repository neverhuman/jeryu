use std::collections::{BTreeMap, HashMap};

pub(super) fn parse_llm_env(contents: &str) -> HashMap<String, String> {
    let mut values = BTreeMap::new();
    for raw in contents.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || !is_env_key(key) {
            continue;
        }
        let mut value = value.trim();
        if let Some(comment_idx) = value.find(" #") {
            value = value[..comment_idx].trim_end();
        }
        let value = value.trim_matches(|ch| ch == '"' || ch == '\'').to_string();
        if !value.is_empty() {
            values.insert(key.to_string(), value);
        }
    }
    values.into_iter().collect()
}

fn is_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn parses_env_without_exposing_values_in_health_type() {
        let parsed = parse_llm_env(
            r#"
            # comment
            export OPENROUTER_API_KEY="sk-test-one"
            EMPTY=
            BAD-KEY=value
            GROQ_API_KEY='groq-test'
            "#,
        );
        assert_eq!(
            parsed.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from(["GROQ_API_KEY".to_string(), "OPENROUTER_API_KEY".to_string()])
        );
        assert_eq!(parsed["OPENROUTER_API_KEY"], "sk-test-one");
    }
}
