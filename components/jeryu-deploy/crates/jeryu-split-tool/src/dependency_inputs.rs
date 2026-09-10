//! Read-only dependency observations; unresolved execution inputs never imply closure.
//! The command exit includes malformed optional inputs. SQLite eligibility callers
//! must inspect per-capability results and independent qualification evidence.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path},
};

const APP: &str = "application-install";
const AUDIT: &str = "sqlite-release-audit";
const MIRRORS: &str = "standalone-mirrors";
const IMAGE: &str = "optional-image";
const REDLINE: &str = "optional-redline";
const REQUIRED: &[&str] = &[APP, AUDIT, MIRRORS];
const ALL: &[&str] = &[APP, AUDIT, MIRRORS, IMAGE, REDLINE];
const INPUT_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Default)]
struct Inventory {
    inputs: BTreeMap<String, Value>,
    observations: Vec<Value>,
    unresolved: Vec<Value>,
    enrollment: Vec<Value>,
    errors: Vec<Value>,
    floors: BTreeMap<String, u8>,
}

fn hash(bytes: &[u8]) -> String {
    crate::audit_evidence::hash(bytes)
}
fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field)?.as_str()
}
fn oid(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn required(caps: &[&str]) -> bool {
    caps.iter().any(|cap| REQUIRED.contains(cap))
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
fn safe_url(value: &str) -> Option<String> {
    // Do not publish userinfo, query credentials, fragments or control characters.
    let (_, rest) = value.split_once("://")?;
    if !matches!(value.split_once("://")?.0, "https" | "http")
        || rest.split('/').next()?.contains('@')
        || value.contains(['?', '#'])
        || value.chars().any(char::is_control)
    {
        return None;
    }
    Some(value.to_owned())
}
fn github_slug(value: &str) -> Option<String> {
    // Ownership aliases do not change the exact source URL stored on an edge.
    let rest = value
        .strip_prefix("https://github.com/")
        .or_else(|| value.strip_prefix("https://www.github.com/"))?;
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if [owner, repo].iter().any(|part| {
        part.is_empty()
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    }) {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}
fn metadata_identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
fn read_input(root: &Path, path: &str) -> Result<String> {
    ensure!(
        relative(path),
        "dependency input path must remain inside source"
    );
    let path = root.join(path);
    ensure!(
        fs::canonicalize(&path)? == path,
        "dependency input must be physical"
    );
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.nlink() == 1 && before.len() <= INPUT_LIMIT,
        "dependency input must be a bounded single-link regular file"
    );
    let mut bytes = Vec::new();
    (&file).take(INPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    let named = fs::symlink_metadata(&path)?;
    ensure!(
        named.is_file()
            && named.nlink() == 1
            && metadata_identity(&before) == metadata_identity(&file.metadata()?)
            && metadata_identity(&before) == metadata_identity(&named)
            && bytes.len() as u64 == before.len(),
        "dependency input changed during read"
    );
    Ok(String::from_utf8(bytes)?)
}

impl Inventory {
    fn gap(&mut self, path: &str, location: &str, reason: &str, caps: &[&str]) {
        self.unresolved
            .push(json!({"path":path,"location":location,"reason":reason,"capabilities":caps}));
    }
    fn error(&mut self, path: &str, location: &str, reason: &str, caps: &[&str]) {
        self.gap(path, location, reason, caps);
        self.errors
            .push(json!({"path":path,"location":location,"reason":reason,"capabilities":caps}));
    }
    fn observe(
        &mut self,
        path: &str,
        location: &str,
        kind: &str,
        consumer: &str,
        caps: &[&str],
        mut facts: Value,
    ) {
        if facts.get("license").is_some_and(Value::is_null) {
            facts["license_status"] = json!("not_supplied_by_input");
        }
        self.observations
            .push(json!({"path":path,"location":location,"kind":kind,
            "consumer":consumer,"capabilities":caps,"facts":facts}));
    }
    fn load(&mut self, root: &Path, path: &str, kind: &str, caps: &[&str]) -> Option<String> {
        match read_input(root, path) {
            Ok(source) => {
                self.inputs.insert(
                    path.into(),
                    json!({"path":path,"kind":kind,
                    "sha256":hash(source.as_bytes()),"capabilities":caps,"status":"read"}),
                );
                Some(source)
            }
            Err(_) => {
                self.inputs.insert(
                    path.into(),
                    json!({"path":path,"kind":kind,
                    "sha256":null,"capabilities":caps,"status":"unavailable"}),
                );
                self.error(
                    path,
                    "file",
                    "input_missing_unsafe_changed_or_invalid_utf8",
                    caps,
                );
                None
            }
        }
    }
    fn parse(&mut self, path: &str, source: &str, format: &str, caps: &[&str]) -> Option<Value> {
        let parsed = if format == "toml" {
            toml::from_str::<toml::Value>(source)
                .ok()
                .and_then(|value| serde_json::to_value(value).ok())
        } else {
            serde_json::from_str::<Value>(source).ok()
        };
        if !parsed.as_ref().is_some_and(Value::is_object) {
            self.error(path, "document", "invalid_structured_input", caps);
            if let Some(input) = self.inputs.get_mut(path) {
                input["status"] = json!("invalid");
            }
            None
        } else {
            parsed
        }
    }
    fn structured(
        &mut self,
        root: &Path,
        path: &str,
        format: &str,
        caps: &[&str],
    ) -> Option<Value> {
        let source = self.load(root, path, format, caps)?;
        self.parse(path, &source, format, caps)
    }
}

#[path = "dependency_input_enrollment.rs"]
mod enrollment;
#[path = "dependency_input_packages.rs"]
mod packages;
#[path = "dependency_input_paths.rs"]
mod paths;
#[path = "dependency_input_pins.rs"]
mod pins;
#[path = "dependency_input_recipes.rs"]
mod recipes;
use paths::{directory_files, existing, member_paths};
#[path = "dependency_input_selection.rs"]
mod selection;

pub(super) fn run(root: &Path) -> Result<()> {
    let report = selection::generate(root)?;
    let failed = report["errors"]
        .as_array()
        .context("inventory error records")?
        .len();
    writeln!(
        std::io::stdout().lock(),
        "{}",
        crate::canonical_json::pretty(report)?
    )?;
    ensure!(
        failed == 0,
        "dependency inventory emitted {failed} malformed-input or enrollment errors"
    );
    Ok(())
}

#[cfg(test)]
#[path = "dependency_inputs_tests.rs"]
mod tests;
