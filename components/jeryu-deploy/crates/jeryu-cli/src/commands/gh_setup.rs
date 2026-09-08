//! Adapter for `jeryu gh-setup`.
//!
//! Renders (and, unless `--print`, writes) a GitHub CLI `hosts.yml` entry that
//! points `gh` at a jeryu server base URL. jeryu serves `GET /user`, so once the
//! host entry is in place `gh auth status` and `gh api user` resolve against
//! jeryu. The host key is the URL's authority (host[:port]); the rendered entry
//! is byte-identical for identical inputs, so re-running is idempotent.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::GhSetupArgs;
use crate::client::{ClientError, ClientResult};
use crate::commands::render;

const DEFAULT_TOKEN_FILE_DISPLAY: &str = "~/.jeryu/secrets/merge-token";

/// The machine-readable summary emitted under `--json`.
#[derive(Debug, Serialize)]
struct GhSetupReport {
    host_key: String,
    base_url: String,
    config_path: String,
    token_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    token_file: Option<String>,
    written: bool,
}

pub(crate) fn run(json: bool, args: GhSetupArgs, out: &mut dyn Write) -> ClientResult<()> {
    let host_key = host_key(&args.host)?;
    let token = resolve_token(&args)?;
    let config = render_hosts_entry(&host_key, &token.value);
    let path = hosts_path(args.path.as_deref())?;

    let written = if args.print {
        false
    } else {
        write_hosts_file(&path, &config)?;
        true
    };

    let report = GhSetupReport {
        host_key: host_key.clone(),
        base_url: args.host.clone(),
        config_path: path.display().to_string(),
        token_source: token.source.report_label(),
        token_file: token.source.token_file_display(),
        written,
    };

    if json {
        return render(out, true, &report, "");
    }

    if args.print {
        writeln!(
            out,
            "# gh hosts.yml entry for jeryu ({})\n# write to: {}\n# {}\n# Stale host repair: {}\n# GitHub.com auth and local Jeryu host auth are separate; do not run gh auth login for Jeryu hosts.\n{}",
            args.host,
            path.display(),
            token.source.output_note(),
            canonical_repair_command(&args.host),
            config.trim_end()
        )
        .ok();
    } else {
        writeln!(
            out,
            "wrote gh host {host_key} -> {} to {}\n{}\nif gh reports auth trouble for this host, rerun {}; GitHub.com auth and local Jeryu host auth are separate, so do not run gh auth login for Jeryu hosts",
            args.host,
            path.display(),
            token.source.output_note(),
            canonical_repair_command(&args.host)
        )
        .ok();
    }
    Ok(())
}

#[derive(Debug)]
struct ResolvedToken {
    value: String,
    source: TokenSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenSource {
    Argument,
    File { display: String },
}

impl TokenSource {
    fn report_label(&self) -> String {
        match self {
            TokenSource::Argument => "explicit --token".to_string(),
            TokenSource::File { display } => format!("token file {display}"),
        }
    }

    fn token_file_display(&self) -> Option<String> {
        match self {
            TokenSource::Argument => None,
            TokenSource::File { display } => Some(display.clone()),
        }
    }

    fn output_note(&self) -> String {
        match self {
            TokenSource::Argument => "token source: explicit --token".to_string(),
            TokenSource::File { display } => format!("token file used: {display}"),
        }
    }
}

fn canonical_repair_command(host: &str) -> String {
    format!("jeryu gh-setup --host {host} --token-file {DEFAULT_TOKEN_FILE_DISPLAY}")
}

fn resolve_token(args: &GhSetupArgs) -> ClientResult<ResolvedToken> {
    if args.token.is_some() {
        return resolve_token_with_default(args, PathBuf::new(), DEFAULT_TOKEN_FILE_DISPLAY);
    }
    let default_path = default_token_file_path()?;
    resolve_token_with_default(args, default_path, DEFAULT_TOKEN_FILE_DISPLAY)
}

fn resolve_token_with_default(
    args: &GhSetupArgs,
    default_path: PathBuf,
    default_display: &str,
) -> ClientResult<ResolvedToken> {
    if let Some(token) = &args.token {
        let token = trimmed_token_value(token);
        if token.is_empty() {
            return Err(ClientError::Invalid(
                "--token must not be empty; pass --token-file or a non-empty --token".into(),
            ));
        }
        return Ok(ResolvedToken {
            value: token,
            source: TokenSource::Argument,
        });
    }

    let (path, display, defaulted) = match &args.token_file {
        Some(raw) => (expand_home(raw)?, raw.clone(), false),
        None => (default_path, default_display.to_string(), true),
    };
    let value = read_token_file(&path, &display, defaulted)?;
    Ok(ResolvedToken {
        value,
        source: TokenSource::File { display },
    })
}

fn trimmed_token_value(raw: &str) -> String {
    raw.trim_end_matches(['\r', '\n']).to_string()
}

fn read_token_file(path: &Path, display: &str, defaulted: bool) -> ClientResult<String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if defaulted && err.kind() == std::io::ErrorKind::NotFound => {
            return Err(ClientError::Invalid(format!(
                "gh host token is unavailable: default token file {DEFAULT_TOKEN_FILE_DISPLAY} was not found; rerun jeryu gh-setup --host <local-jeryu-url> --token-file {DEFAULT_TOKEN_FILE_DISPLAY} after provisioning it, or pass --token <token>. GitHub.com auth and local Jeryu host auth are separate; do not run gh auth login for Jeryu hosts"
            )));
        }
        Err(err) => {
            return Err(ClientError::Invalid(format!(
                "cannot read gh token file {display}: {err}; pass --token-file <path> or --token <token>"
            )));
        }
    };
    let token = trimmed_token_value(&raw);
    if token.trim().is_empty() {
        return Err(ClientError::Invalid(format!(
            "gh token file {display} is empty; pass --token-file <path> or --token <token>"
        )));
    }
    Ok(token)
}

fn expand_home(raw: &str) -> ClientResult<PathBuf> {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(raw))
}

fn default_token_file_path() -> ClientResult<PathBuf> {
    Ok(home_dir()?
        .join(".jeryu")
        .join("secrets")
        .join("merge-token"))
}

fn home_dir() -> ClientResult<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| ClientError::Invalid("cannot determine HOME for gh token file".into()))
}

/// Derive the `gh` host key (authority) from a base URL. `gh` keys hosts by
/// `host[:port]`, never by scheme or path, so we strip both ends.
fn host_key(base_url: &str) -> ClientResult<String> {
    let after_scheme = base_url
        .split_once("://")
        .map_or(base_url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme)
        .trim_end_matches('/');
    if authority.is_empty() {
        return Err(ClientError::Invalid(format!(
            "cannot derive a gh host key from base url {base_url:?}"
        )));
    }
    Ok(authority.to_string())
}

/// Render the canonical `hosts.yml` entry. Stable field order => idempotent.
fn render_hosts_entry(host_key: &str, token: &str) -> String {
    format!(
        "{host_key}:\n    oauth_token: {token}\n    git_protocol: https\n    users:\n        jeryu:\n            oauth_token: {token}\n    user: jeryu\n"
    )
}

/// The hosts.yml path: an explicit override, else the GitHub CLI default
/// (`$GH_CONFIG_DIR` or `$XDG_CONFIG_HOME/gh` or `$HOME/.config/gh`).
fn hosts_path(override_path: Option<&str>) -> ClientResult<PathBuf> {
    if let Some(p) = override_path {
        return Ok(PathBuf::from(p));
    }
    let dir = match std::env::var_os("GH_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir).join("gh"),
            None => match std::env::var_os("HOME") {
                Some(home) => PathBuf::from(home).join(".config").join("gh"),
                None => {
                    return Err(ClientError::Invalid(
                        "cannot determine the gh config directory".into(),
                    ));
                }
            },
        },
    };
    Ok(dir.join("hosts.yml"))
}

fn write_hosts_file(path: &PathBuf, config: &str) -> ClientResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ClientError::Invalid(format!("create {}: {e}", parent.display())))?;
    }
    std::fs::write(path, config)
        .map_err(|e| ClientError::Invalid(format!("write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_strips_scheme_port_and_path() {
        assert_eq!(
            host_key("https://forge.example:9000/api").unwrap(),
            "forge.example:9000"
        );
        assert_eq!(host_key("http://localhost:8080").unwrap(), "localhost:8080");
        assert_eq!(host_key("forge.example").unwrap(), "forge.example");
    }

    #[test]
    fn host_key_rejects_empty_authority() {
        assert!(host_key("https://").is_err());
    }

    #[test]
    fn rendered_entry_is_idempotent_and_contains_token() {
        let a = render_hosts_entry("localhost:8080", "T");
        let b = render_hosts_entry("localhost:8080", "T");
        assert_eq!(a, b, "identical inputs must render identically");
        assert!(a.contains("oauth_token: T"));
        assert!(a.starts_with("localhost:8080:\n"));
    }

    #[test]
    fn token_resolution_prefers_explicit_token_over_missing_file() {
        let args = GhSetupArgs {
            host: "http://localhost:8080".to_string(),
            token: Some("explicit-token".to_string()),
            token_file: Some("/definitely/missing/token".to_string()),
            print: false,
            path: None,
        };

        let resolved = resolve_token_with_default(
            &args,
            PathBuf::from("/definitely/missing/default"),
            DEFAULT_TOKEN_FILE_DISPLAY,
        )
        .expect("explicit token wins");

        assert_eq!(resolved.value, "explicit-token");
        assert_eq!(resolved.source, TokenSource::Argument);
    }

    #[test]
    fn token_resolution_uses_default_token_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let token_path = dir.path().join("merge-token");
        std::fs::write(&token_path, "file-token\n").expect("write token");
        let args = GhSetupArgs {
            host: "http://localhost:8080".to_string(),
            token: None,
            token_file: None,
            print: false,
            path: None,
        };

        let resolved = resolve_token_with_default(&args, token_path, DEFAULT_TOKEN_FILE_DISPLAY)
            .expect("default token file");

        assert_eq!(resolved.value, "file-token");
        assert_eq!(
            resolved.source,
            TokenSource::File {
                display: DEFAULT_TOKEN_FILE_DISPLAY.to_string()
            }
        );
    }

    #[test]
    fn missing_default_token_file_guides_without_token_placeholder() {
        let args = GhSetupArgs {
            host: "http://localhost:8080".to_string(),
            token: None,
            token_file: None,
            print: false,
            path: None,
        };

        let err = resolve_token_with_default(
            &args,
            PathBuf::from("/definitely/missing/default-token"),
            DEFAULT_TOKEN_FILE_DISPLAY,
        )
        .expect_err("missing default token file fails");
        let message = err.to_string();

        assert!(message.contains(DEFAULT_TOKEN_FILE_DISPLAY));
        assert!(message.contains("--token-file"));
        assert!(message.contains("GitHub.com auth and local Jeryu host auth are separate"));
        assert!(!message.contains("JERYU-TOKEN"));
    }
}
