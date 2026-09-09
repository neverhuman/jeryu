//! Shared JSON reader and Python-`repr()`-compatible rendering helpers.
//!
//! The diagnostic strings these helpers produce must remain byte-identical to
//! the messages the legacy `check-*.py` scripts printed, so every gate that
//! formats a list or an entry funnels through here.

use std::path::Path;

use anyhow::{Context, Result};

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing {} as JSON", path.display()))
}

/// Render a list of strings the way Python prints `['a', 'b']`.
pub(crate) fn py_str_list(items: &[&str]) -> String {
    let inner = items
        .iter()
        .map(|s| py_repr(s))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{inner}]")
}

/// Render a Python `repr()` of a string: single-quoted.
pub(crate) fn py_repr(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Render a Python `repr()` of an optional string (`None` when absent).
pub(crate) fn py_repr_opt(s: Option<&str>) -> String {
    match s {
        Some(v) => py_repr(v),
        None => "None".to_string(),
    }
}

pub(crate) fn py_str_list_owned(items: &[String]) -> String {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    py_str_list(&refs)
}
