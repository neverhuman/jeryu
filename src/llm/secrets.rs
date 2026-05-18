//! 6-tier secret lookup chain for LLM provider keys.
//!
//! Precedence (first hit wins):
//!   1. Explicit `--llm-key <PROVIDER>=<VALUE>` CLI flag (caller-supplied).
//!   2. Process env var.
//!   3. `~/.jeryu/secrets/llm.env` (dotenvy; canonical user-default).
//!   4. `~/llm.env`  (legacy / current user-provided path; supported for back-compat).
//!   5. `.env.local` at repo root (gitignored).
//!   6. CI secret (env var injected by GitHub/GitLab CI — handled by tier #2 in CI).
//!
//! In CI mode (`CI=true`), local file tiers (3, 4, 5) are skipped to avoid
//! accidentally reading developer keys.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    Cli,
    Env,
    UserDefault,
    UserLegacy,
    RepoLocal,
    NotFound,
}

#[derive(Debug, Clone)]
pub struct ResolvedSecret {
    pub source: SecretSource,
    pub value: String,
}

#[derive(Debug, Default, Clone)]
pub struct SecretResolver {
    pub cli_overrides: HashMap<String, String>,
    pub repo_root: Option<PathBuf>,
    pub ci_mode: bool,
}

impl SecretResolver {
    pub fn from_env() -> Self {
        Self {
            cli_overrides: HashMap::new(),
            repo_root: std::env::current_dir().ok(),
            ci_mode: std::env::var("CI")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        }
    }
}

/// Resolve a named secret (e.g. `OPENROUTER_API_KEY`) using the 6-tier chain.
pub fn resolve_secret(name: &str, resolver: &SecretResolver) -> Option<ResolvedSecret> {
    if let Some(v) = resolver.cli_overrides.get(name) {
        return Some(ResolvedSecret {
            source: SecretSource::Cli,
            value: v.clone(),
        });
    }
    if let Ok(v) = std::env::var(name)
        && !v.is_empty()
    {
        return Some(ResolvedSecret {
            source: SecretSource::Env,
            value: v,
        });
    }
    if resolver.ci_mode {
        return None;
    }
    if let Some(home) = dirs::home_dir() {
        let user_default = home.join(".jeryu/secrets/llm.env");
        if let Some(v) = read_env_value(&user_default, name) {
            return Some(ResolvedSecret {
                source: SecretSource::UserDefault,
                value: v,
            });
        }
        let legacy = home.join("llm.env");
        if let Some(v) = read_env_value(&legacy, name) {
            return Some(ResolvedSecret {
                source: SecretSource::UserLegacy,
                value: v,
            });
        }
    }
    if let Some(root) = &resolver.repo_root {
        let local = root.join(".env.local");
        if let Some(v) = read_env_value(&local, name) {
            return Some(ResolvedSecret {
                source: SecretSource::RepoLocal,
                value: v,
            });
        }
    }
    None
}

fn read_env_value(path: &std::path::Path, key: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cli_override_beats_everything() {
        let mut r = SecretResolver::from_env();
        r.cli_overrides.insert("FOO".into(), "from_cli".into());
        // Even if env is set, cli wins.
        // SAFETY: Rust 2024 marks env mutation unsafe due to unsynchronized
        // reads. Tests run serially (`--test-threads=1` per pre-pr.sh) and
        // we restore the var below.
        unsafe {
            std::env::set_var("FOO", "from_env");
        }
        let s = resolve_secret("FOO", &r).unwrap();
        assert_eq!(s.source, SecretSource::Cli);
        assert_eq!(s.value, "from_cli");
        // SAFETY: serialized as above.
        unsafe {
            std::env::remove_var("FOO");
        }
    }

    #[test]
    fn env_var_resolves_when_set() {
        let r = SecretResolver {
            ci_mode: true,
            ..Default::default()
        };
        // SAFETY: Rust 2024 marks env mutation unsafe; tests serialize via
        // `--test-threads=1` and we restore the var at the end of the test.
        unsafe {
            std::env::set_var("JERYU_LLM_TEST_K", "hello");
        }
        let s = resolve_secret("JERYU_LLM_TEST_K", &r).unwrap();
        assert_eq!(s.source, SecretSource::Env);
        assert_eq!(s.value, "hello");
        // SAFETY: serialized as above.
        unsafe {
            std::env::remove_var("JERYU_LLM_TEST_K");
        }
    }

    #[test]
    fn ci_mode_skips_local_files() {
        let r = SecretResolver {
            ci_mode: true,
            ..Default::default()
        };
        // Even if ~/llm.env exists with the key, CI mode must not read it.
        let result = resolve_secret("__JERYU_NEVER_DEFINED__", &r);
        assert!(result.is_none());
    }

    #[test]
    fn read_env_value_parses_quoted_and_unquoted() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f, "FOO=bar").unwrap();
        writeln!(f, "BAZ=\"quoted value\"").unwrap();
        writeln!(f, "EMPTY=").unwrap();
        let p = f.path();
        assert_eq!(read_env_value(p, "FOO"), Some("bar".into()));
        assert_eq!(read_env_value(p, "BAZ"), Some("quoted value".into()));
        assert_eq!(read_env_value(p, "EMPTY"), None);
        assert_eq!(read_env_value(p, "MISSING"), None);
    }
}
