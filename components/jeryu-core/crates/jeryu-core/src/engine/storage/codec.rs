use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::errors::ForgeError;

use super::{Result, storage_error};

pub(super) fn bool_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

pub(super) fn int_bool(value: i64) -> bool {
    value != 0
}

pub(super) fn time(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

pub(super) fn optional_time(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(time)
}

pub(super) fn parse_time(value: String) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(storage_error)
}

pub(super) fn parse_optional_time(value: Option<String>) -> Result<Option<DateTime<Utc>>> {
    value.map(parse_time).transpose()
}

pub(super) fn parse_uuid(value: String) -> Result<Uuid> {
    Uuid::parse_str(&value).map_err(storage_error)
}

pub(super) fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(storage_error)
}

pub(super) fn optional_json<T: Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value.as_ref().map(json).transpose()
}

pub(super) fn parse_json<T: DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_str(&value).map_err(storage_error)
}

pub(super) fn parse_optional_json<T: DeserializeOwned>(value: Option<String>) -> Result<Option<T>> {
    value.map(parse_json).transpose()
}

pub(super) fn text<T: Serialize>(value: &T) -> Result<String> {
    match serde_json::to_value(value).map_err(storage_error)? {
        Value::String(value) => Ok(value),
        other => Err(ForgeError::Storage(format!(
            "expected enum string, got {other}"
        ))),
    }
}

pub(super) fn optional_text<T: Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value.as_ref().map(text).transpose()
}

pub(super) fn from_text<T: DeserializeOwned>(value: String) -> Result<T> {
    serde_json::from_value(Value::String(value)).map_err(storage_error)
}

pub(super) fn from_optional_text<T: DeserializeOwned>(value: Option<String>) -> Result<Option<T>> {
    value.map(from_text).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CheckRunStatus, IssueState, ReviewState};

    #[test]
    fn enum_text_uses_wire_shape() {
        assert_eq!(text(&IssueState::Closed).unwrap(), "closed");
        assert_eq!(text(&CheckRunStatus::InProgress).unwrap(), "in_progress");
        assert_eq!(text(&ReviewState::Approved).unwrap(), "APPROVED");
    }
}
