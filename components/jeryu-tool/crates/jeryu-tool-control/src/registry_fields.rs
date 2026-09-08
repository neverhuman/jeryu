use regex::Regex;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub(super) const VALID_KINDS: [&str; 6] = [
    "rust-crate",
    "ts-lib",
    "react-component",
    "vite-plugin",
    "shell-lib",
    "jankurai-tool",
];
pub(super) const VALID_TOOL_STATUS: [&str; 4] = ["proposed", "building", "published", "deprecated"];
pub(super) const VALID_TASK_STATUS: [&str; 3] = ["open", "in-progress", "done"];
pub(super) const TOP_LEVEL_FIELDS: [&str; 2] = ["schema_version", "tool"];
const TOOL_FIELDS: [&str; 10] = [
    "id",
    "name",
    "kind",
    "status",
    "source",
    "description",
    "adopting_repos",
    "candidate_repos",
    "loc_saved",
    "loc_saved_estimate",
];
const DISCOVERED_TOOL_FIELDS: [&str; 11] = [
    "id",
    "name",
    "kind",
    "status",
    "source",
    "description",
    "origin_cluster",
    "adopting_repos",
    "candidate_repos",
    "loc_saved",
    "loc_saved_estimate",
];
pub(super) const TASK_FIELDS: [&str; 8] = [
    "id",
    "tool_id",
    "title",
    "status",
    "origin_cluster",
    "anticipated_loc_saved",
    "target_repos",
    "rollout",
];

pub(super) fn fail(message: impl AsRef<str>) -> String {
    format!("tools-registry: {}", message.as_ref())
}

pub(super) fn string<'a>(
    table: &'a toml::Table,
    field: &str,
    context: &str,
) -> Result<&'a str, String> {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| fail(format!("{context} field {field:?} must be a string")))
}

pub(super) fn strings(
    table: &toml::Table,
    field: &str,
    context: &str,
) -> Result<Vec<String>, String> {
    let values = table
        .get(field)
        .and_then(toml::Value::as_array)
        .ok_or_else(|| fail(format!("{context} field {field:?} must be a list")))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| fail(format!("{context} field {field:?} must contain strings")))
        })
        .collect()
}

pub(super) fn integer(table: &toml::Table, field: &str, context: &str) -> Result<u64, String> {
    let value = table
        .get(field)
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| {
            fail(format!(
                "{context} field {field:?} must be a non-negative integer"
            ))
        })?;
    u64::try_from(value).map_err(|_| {
        fail(format!(
            "{context} field {field:?} must be a non-negative integer"
        ))
    })
}

pub(super) fn exact_keys(
    table: &toml::Table,
    expected: &[&str],
    context: &str,
) -> Result<(), String> {
    let actual: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = expected.iter().copied().collect();
    if actual != expected {
        let missing: Vec<&str> = expected.difference(&actual).copied().collect();
        let unknown: Vec<&str> = actual.difference(&expected).copied().collect();
        return Err(fail(format!(
            "{context} key set mismatch: missing={missing:?} unknown={unknown:?}"
        )));
    }
    Ok(())
}

pub(super) fn exact_tool_keys(table: &toml::Table, context: &str) -> Result<(), String> {
    let actual: BTreeSet<&str> = table.keys().map(String::as_str).collect();
    let ordinary: BTreeSet<&str> = TOOL_FIELDS.iter().copied().collect();
    let discovered: BTreeSet<&str> = DISCOVERED_TOOL_FIELDS.iter().copied().collect();
    if actual == ordinary || actual == discovered {
        return Ok(());
    }
    let allowed: BTreeSet<&str> = ordinary.union(&discovered).copied().collect();
    let missing: Vec<&str> = ordinary.difference(&actual).copied().collect();
    let unknown: Vec<&str> = actual.difference(&allowed).copied().collect();
    Err(fail(format!(
        "{context} key set mismatch: missing={missing:?} unknown={unknown:?}"
    )))
}

pub(super) fn nonempty_text(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

pub(super) fn valid_slug(value: &str) -> bool {
    Regex::new(r"^[a-z0-9][a-z0-9-]{0,127}$")
        .expect("constant regex")
        .is_match(value)
}

pub(super) fn valid_origin_cluster(value: &str) -> bool {
    value.strip_prefix("toolbuild-").is_some_and(|digest| {
        digest.len() == 16
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}

pub(super) fn validate_unique_repos(values: &[String], context: &str) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !valid_slug(value) {
            return Err(fail(format!(
                "{context} contains invalid repository {value:?}"
            )));
        }
        if !unique.insert(value) {
            return Err(fail(format!(
                "{context} contains duplicate repository {value:?}"
            )));
        }
    }
    Ok(())
}

pub(super) fn load_toml(path: &Path) -> Result<toml::Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| fail(format!("{}: {error}", path.display())))?;
    toml::from_str(&text).map_err(|error| fail(format!("{}: {error}", path.display())))
}
