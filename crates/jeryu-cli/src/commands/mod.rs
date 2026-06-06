//! Thin command adapters: each maps a parsed clap leaf onto a
//! [`crate::client::ForgeClient`] call and renders the result.
//!
//! Adapters are output-agnostic: they write to a `&mut dyn Write` so the
//! dispatch smoke tests can capture and assert on rendered output without
//! touching the process stdout.

pub mod agent;
pub mod api;
pub mod autonomy;
pub mod cache;
pub mod ci;
pub mod control_plane;
pub mod forge;
pub mod gh_setup;
pub mod onboard;
pub mod proof;
pub mod release;
pub mod runner;

use std::io::Write;

use serde::Serialize;

use crate::client::ClientResult;

/// Render either a JSON value or a human line, depending on `json`.
pub(crate) fn render<T: Serialize>(
    out: &mut dyn Write,
    json: bool,
    value: &T,
    human: &str,
) -> ClientResult<()> {
    if json {
        let text = serde_json::to_string(value)
            .map_err(|e| crate::client::ClientError::Invalid(format!("serialize: {e}")))?;
        writeln!(out, "{text}").ok();
    } else {
        writeln!(out, "{human}").ok();
    }
    Ok(())
}
