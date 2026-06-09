//! Adapter for `jeryu gh-setup`.
//!
//! Renders (and, unless `--print`, writes) a GitHub CLI `hosts.yml` entry that
//! points `gh` at a jeryu server base URL. jeryu serves `GET /user`, so once the
//! host entry is in place `gh auth status` and `gh api user` resolve against
//! jeryu. The host key is the URL's authority (host[:port]); the rendered entry
//! is byte-identical for identical inputs, so re-running is idempotent.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use crate::cli::GhSetupArgs;
use crate::client::{ClientError, ClientResult};
use crate::commands::render;

/// The machine-readable summary emitted under `--json`.
#[derive(Debug, Serialize)]
struct GhSetupReport {
    host_key: String,
    base_url: String,
    config_path: String,
    written: bool,
    config: String,
}

pub(crate) fn run(json: bool, args: GhSetupArgs, out: &mut dyn Write) -> ClientResult<()> {
    let host_key = host_key(&args.host)?;
    let config = render_hosts_entry(&host_key, &args.token);
    let path = hosts_path(args.path.as_deref());

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
        written,
        config: config.clone(),
    };

    if json {
        return render(out, true, &report, "");
    }

    if args.print {
        writeln!(
            out,
            "# gh hosts.yml entry for jeryu ({})\n# write to: {}\n# If gh reports auth trouble for this host, rerun jeryu gh-setup; do not run gh auth login or gh auth refresh.\n{}",
            args.host,
            path.display(),
            config.trim_end()
        )
        .ok();
    } else {
        writeln!(
            out,
            "wrote gh host {host_key} -> {} to {}\nif gh reports auth trouble for this host, rerun jeryu gh-setup; do not run gh auth login or gh auth refresh",
            args.host,
            path.display()
        )
        .ok();
    }
    Ok(())
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
fn hosts_path(override_path: Option<&str>) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }
    let dir = std::env::var_os("GH_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|x| PathBuf::from(x).join("gh")))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("gh")))
        .unwrap_or_else(|| PathBuf::from(".config").join("gh"));
    dir.join("hosts.yml")
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
}
