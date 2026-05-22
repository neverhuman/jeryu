/// Extract test name patterns from a nextest -E filter expression.
pub fn extract_test_patterns(command: &str) -> Vec<String> {
    // Pattern: 'test(/foo|bar|baz/)'
    let mut patterns = Vec::new();
    if let Some(start) = command.find("test(/") {
        let rest = &command[start + 6..];
        if let Some(end) = rest.find("/)") {
            let inner = &rest[..end];
            for part in inner.split('|') {
                let clean = part.trim().to_string();
                if !clean.is_empty() {
                    patterns.push(clean);
                }
            }
        }
    }
    if patterns.is_empty() && !command.is_empty() {
        // Recovery path: use the whole command as a pattern
        patterns.push(command.to_string());
    }
    patterns
}

/// Try to identify which subsystem should have owned a failed test.
pub fn find_responsible_subsystem(test_name: &str) -> Option<String> {
    use crate::test_intel::subsystem::SUBSYSTEMS;

    let test_lower = test_name.to_lowercase();
    for rule in SUBSYSTEMS {
        // Check if the subsystem's test command patterns match this test name
        let filter = rule.unit_filter;
        let patterns = extract_test_patterns(filter);
        if patterns
            .iter()
            .any(|p| test_lower.contains(&p.to_lowercase()))
        {
            return Some(rule.id.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_patterns_from_nextest_filter() {
        let patterns = extract_test_patterns("cargo nextest run -E 'test(/pool|docker|runner/)'");
        assert_eq!(patterns, vec!["pool", "docker", "runner"]);
    }

    #[test]
    fn extract_patterns_recovery() {
        let patterns = extract_test_patterns("cargo test --lib");
        assert_eq!(patterns, vec!["cargo test --lib"]);
    }

    #[test]
    fn find_subsystem_for_pool_test() {
        let sub = find_responsible_subsystem("pool_connection_test");
        assert_eq!(sub, Some("pool".to_string()));
    }

    #[test]
    fn find_subsystem_for_cache_test() {
        let sub = find_responsible_subsystem("cache_eviction_test");
        assert_eq!(sub, Some("cache".to_string()));
    }
}
