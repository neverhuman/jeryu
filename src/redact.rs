//! Owner: cross-cutting redaction helpers
//! Proof: `cargo test -p jeryu redact`
//! Invariants: Sensitive tokens are redacted before they reach receipts, logs, or human-facing summaries.

use sha2::{Digest, Sha256};

pub fn redact_text(input: &str) -> String {
    let mut out = input.to_string();
    for (needle, replacement) in [("https://", "https://"), ("http://", "http://")] {
        if out.contains(needle) {
            out = redact_urls(&out);
        }
        let _ = replacement;
    }
    out = redact_basic_auth(&out);
    out = redact_known_tokens(&out);
    out
}

pub fn redact_args(args: &[String]) -> Vec<String> {
    args.iter().map(|arg| redact_text(arg)).collect()
}

pub fn hash_argv(args: &[String]) -> String {
    let mut hasher = Sha256::new();
    for arg in args {
        hasher.update(arg.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn redact_urls(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            if token.starts_with("http://") || token.starts_with("https://") {
                redact_url(token)
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn redact_url(url: &str) -> String {
    if let Some((scheme, rest)) = url.split_once("://") {
        let mut out = String::from(scheme);
        out.push_str("://");
        if let Some((auth, tail)) = rest.split_once('@') {
            let host = tail.split('/').next().unwrap_or("");
            out.push_str("redacted@");
            out.push_str(host);
            if let Some(path) = tail.strip_prefix(host) {
                out.push_str(path);
            }
            if auth.contains(':') {
                return out;
            }
        }
    }
    url.to_string()
}

fn redact_basic_auth(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            if let Some((scheme, rest)) = token.split_once("://")
                && let Some((auth, tail)) = rest.split_once('@')
                && auth.contains(':')
            {
                let host = tail.split('/').next().unwrap_or("");
                let suffix = tail.strip_prefix(host).unwrap_or("");
                return format!("{scheme}://redacted@{host}{suffix}");
            }
            token.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_known_tokens(input: &str) -> String {
    let mut out = input.to_string();
    for marker in ["token=", "access_token=", "password=", "pat="] {
        if let Some(idx) = out.to_ascii_lowercase().find(marker) {
            let end = out[idx..]
                .find([' ', '&', '\n', '\r'])
                .map(|n| idx + n)
                .unwrap_or(out.len());
            out.replace_range(idx..end, &format!("{marker}redacted"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_basic_auth_urls() {
        let redacted = redact_text("https://user:secret@example.com/path");
        assert!(redacted.contains("redacted@example.com"));
        assert!(!redacted.contains("secret"));
    }

    #[test]
    fn hashes_argv_stably() {
        let left = hash_argv(&["git".into(), "status".into()]);
        let right = hash_argv(&["git".into(), "status".into()]);
        assert_eq!(left, right);
    }
}
