//! Pre-flight secret scrub: refuses to send any byte to an LLM provider when a
//! candidate secret is flagged in the diff.
//!
//! Always fail-closed unless `JERYU_LLM_SCRUB_SKIP=1` (opt-in for unit tests
//! only — never a CI default).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubFinding {
    pub kind: String,
    pub line_offset: usize,
    pub matched_snippet: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScrubReport {
    pub passed: bool,
    pub findings: Vec<ScrubFinding>,
    pub tool: &'static str,
}

/// Scrub a diff for embedded secrets. In fail-closed default, any finding
/// aborts the LLM call.
pub fn scrub_diff(diff: &str) -> ScrubReport {
    if std::env::var("JERYU_LLM_SCRUB_SKIP").as_deref() == Ok("1") {
        return ScrubReport {
            passed: true,
            findings: vec![],
            tool: "skipped",
        };
    }
    let findings = scan_pure_rust(diff);
    ScrubReport {
        passed: findings.is_empty(),
        findings,
        tool: "regex-scanner",
    }
}

/// Lightweight, dependency-light secret scanner covering common shapes.
fn scan_pure_rust(diff: &str) -> Vec<ScrubFinding> {
    let patterns: &[(&str, &str)] = &[
        ("aws-access-key-id", r"AKIA[0-9A-Z]{16}"),
        ("github-pat", r"github_pat_[A-Za-z0-9_]{40,}"),
        ("openai-key", r"sk-[A-Za-z0-9]{30,}"),
        ("openrouter-key", r"sk-or-v1-[A-Za-z0-9]{40,}"),
        ("groq-key", r"gsk_[A-Za-z0-9]{40,}"),
        ("gemini-key", r"AIza[0-9A-Za-z_\-]{30,}"),
        ("anthropic-key", r"sk-ant-[A-Za-z0-9_\-]{40,}"),
        ("nvidia-key", r"nvapi-[A-Za-z0-9_\-]{40,}"),
        ("fireworks-key", r"fw_[A-Za-z0-9_\-]{20,}"),
        ("cerebras-key", r"csk-[A-Za-z0-9_\-]{30,}"),
        ("hf-token", r"hf_[A-Za-z0-9]{30,}"),
        ("slack-bot-token", r"xox[baprs]-[A-Za-z0-9\-]{10,}"),
        (
            "private-key-pem",
            r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----",
        ),
        (
            "jwt",
            r"eyJ[A-Za-z0-9_\-]{8,}\.eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
        ),
    ];
    let mut findings = Vec::new();
    for (idx, line) in diff.lines().enumerate() {
        for (kind, pat) in patterns {
            if let Ok(re) = regex::Regex::new(pat)
                && let Some(m) = re.find(line)
            {
                let snippet = m.as_str();
                let redacted = if snippet.len() > 8 {
                    format!(
                        "{}…{} ({} chars)",
                        &snippet[..4],
                        &snippet[snippet.len().saturating_sub(2)..],
                        snippet.len()
                    )
                } else {
                    "<redacted>".to_string()
                };
                findings.push(ScrubFinding {
                    kind: (*kind).to_string(),
                    line_offset: idx,
                    matched_snippet: redacted,
                });
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn clean_diff_passes() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("JERYU_LLM_SCRUB_SKIP");
        }
        let diff = "+ fn add(a: i32, b: i32) -> i32 { a + b }";
        let r = scrub_diff(diff);
        assert!(r.passed);
        assert!(r.findings.is_empty());
    }

    #[test]
    fn aws_key_is_caught() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("JERYU_LLM_SCRUB_SKIP");
        }
        let diff = format!("+ const KEY: &str = \"{}{}\";", "AKIA", "IOSFODNN7EXAMPLE");
        let r = scrub_diff(&diff);
        assert!(!r.passed);
        assert_eq!(r.findings[0].kind, "aws-access-key-id");
    }

    #[test]
    fn skip_env_var_bypasses() {
        let _g = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("JERYU_LLM_SCRUB_SKIP", "1");
        }
        let diff = format!("+ const KEY: &str = \"{}{}\";", "AKIA", "IOSFODNN7EXAMPLE");
        let r = scrub_diff(&diff);
        assert!(r.passed);
        assert_eq!(r.tool, "skipped");
        unsafe {
            std::env::remove_var("JERYU_LLM_SCRUB_SKIP");
        }
    }
}
