//! Small JSON [`Value`] accessors shared by the GitHub export parsers.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::model::*;

pub(super) fn array<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    match value.get(key).and_then(Value::as_array) {
        Some(items) => items.iter().collect(),
        None => Vec::new(),
    }
}

pub(super) fn strings_from_array(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

pub(super) fn string(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

pub(super) fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub(super) fn parse_state(value: Option<&str>) -> ObjectState {
    match value.unwrap_or("unknown").to_ascii_lowercase().as_str() {
        "open" | "opened" => ObjectState::Open,
        "closed" => ObjectState::Closed,
        "merged" => ObjectState::Merged,
        "draft" => ObjectState::Draft,
        "archived" => ObjectState::Archived,
        _ => ObjectState::Unknown,
    }
}

pub(super) fn parse_time(value: Option<&str>) -> Option<DateTime<Utc>> {
    let Some(text) = value else {
        // Field absent in the export: there is genuinely no timestamp to record.
        return None;
    };
    match DateTime::parse_from_rfc3339(text) {
        Ok(time) => Some(time.with_timezone(&Utc)),
        Err(_) => {
            // A present-but-malformed timestamp is treated as "unknown time" rather
            // than aborting the whole import: GitHub exports occasionally carry
            // non-RFC3339 or empty timestamp strings, and a single bad field must
            // not discard an otherwise-valid repository archive. The lossy branch
            // is taken explicitly so it is not mistaken for the absent-field case.
            None
        }
    }
}
