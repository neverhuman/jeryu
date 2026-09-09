//! Key-value parsing helpers and field validation for job requests.

use crate::error::{RunnerError, RunnerResult};
use crate::fscheck::deny_dangerous_host_path;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn required(
    values: &BTreeMap<String, String>,
    key: &'static str,
) -> RunnerResult<String> {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| RunnerError::new("missing_job_field", format!("missing {key}")))
}

pub(super) fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn parse_u64(field: &'static str, value: &str) -> RunnerResult<u64> {
    value.trim().parse::<u64>().map_err(|err| {
        RunnerError::new(
            "invalid_integer",
            format!("{field} must be an unsigned integer: {err}"),
        )
    })
}

pub(super) fn parse_bool(field: &'static str, value: &str) -> RunnerResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(RunnerError::new(
            "invalid_bool",
            format!("{field} must be true/false"),
        )),
    }
}

pub(super) fn validate_env_name(name: &str) -> RunnerResult<()> {
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && !name.chars().next().is_some_and(|ch| ch.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(RunnerError::new(
            "invalid_env_name",
            format!("invalid environment variable '{name}'"),
        ))
    }
}

pub(super) fn validate_workspace_path(path: &Path) -> RunnerResult<()> {
    if !path.is_absolute() {
        return Err(RunnerError::new(
            "invalid_workspace",
            format!("workspace '{}' must be absolute", path.display()),
        ));
    }
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(RunnerError::new(
            "invalid_workspace",
            format!("workspace '{}' must not contain '..'", path.display()),
        ));
    }
    deny_dangerous_host_path(path)?;
    Ok(())
}
