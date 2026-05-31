//! Deterministic hashing, id sanitisation, and canonical-string helpers.

use std::fmt;

use crate::{NetworkPolicy, TokenScope};

/// Deterministic FNV-1a 64-bit hash, rendered as a stable `fnv64:` string.
pub fn deterministic_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}

/// Stable, prefixed identifier derived from a value's deterministic hash.
pub fn stable_id(prefix: &str, value: &str) -> String {
    let hash = deterministic_hash(value).replace("fnv64:", "");
    format!("{prefix}_{hash}")
}

/// Normalises an arbitrary label into a deterministic, filesystem-safe id.
pub fn sanitize_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

/// Strips a single pair of matching surrounding quotes from a trimmed value.
pub fn trim_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

pub(crate) fn normalize_token(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn line<K: fmt::Display, V: fmt::Display>(out: &mut String, key: K, value: V) {
    out.push_str(&key.to_string());
    out.push('=');
    out.push_str(&escape_canonical(&value.to_string()));
    out.push('\n');
}

fn escape_canonical(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

pub(crate) fn network_to_string(policy: &NetworkPolicy) -> String {
    match policy {
        NetworkPolicy::Deny => "deny".to_string(),
        NetworkPolicy::Allowlist(hosts) => format!("allowlist:{}", hosts.join(",")),
        NetworkPolicy::Open => "open".to_string(),
    }
}

pub(crate) fn token_to_string(scope: &TokenScope) -> String {
    match scope {
        TokenScope::None => "none".to_string(),
        TokenScope::ReadRepo => "read-repo".to_string(),
        TokenScope::WriteChecks => "write-checks".to_string(),
        TokenScope::WritePullRequest => "write-pull-request".to_string(),
        TokenScope::Custom(values) => format!("custom:{}", values.join(",")),
    }
}
