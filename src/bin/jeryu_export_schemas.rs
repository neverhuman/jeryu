//! `jeryu_export_schemas` — regenerate OpenAPI + JSON-Schema artifacts.
//!
//! Outputs:
//! - `schemas/web-api.openapi.json` — OpenAPI 3.x spec for `/api/v1/*` HTTP
//!   routes, generated from `#[utoipa::path]` on the BFF handlers and
//!   `#[derive(ToSchema)]` on the DTOs.
//! - `schemas/websocket-events.schema.json` — JSON Schema for the
//!   WebSocket frame envelopes (`ClientWsMessage`, `ServerWsMessage`,
//!   `WebEvent`), generated from schemars.
//!
//! Registered as a generated zone in `agent/generated-zones.toml`. The
//! bin is `--features web`-only because the `utoipa::ToSchema` and
//! `schemars::JsonSchema` derives on the DTOs are gated on `feature = "web"`.
//!
//! ## Design note
//!
//! The OpenAPI `paths` block references handler functions that live
//! binary-side under `crate::web::rest::*` (the `web`, `repos`, and `merge`
//! modules are tied to per-process `Arc<GitLabClient>` + `WebEventBus` state
//! and so are NOT in the `jeryu` library crate). A standalone bin cannot
//! see those modules, so this bin is a thin wrapper that shells out to
//! `cargo run --bin jeryu --features web -- web export-schemas` (which
//! lives in `src/web/openapi.rs` + `src/web/command.rs`).
//!
//! The double `cargo run` invocation reuses the build cache; the inner
//! call is the one that actually emits the JSON. Set
//! `JERYU_SCHEMA_OUT_DIR` to redirect output; the default is `schemas`.

#![cfg(feature = "web")]

use std::path::PathBuf;
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = schema_export_config();
    let status = Command::new(&config.cargo_bin)
        .args([
            "run",
            "--quiet",
            "--bin",
            "jeryu",
            "--features",
            "web",
            "--",
            "web",
            "export-schemas",
            "--out-dir",
            config.out_dir.to_str().unwrap_or("schemas"),
        ])
        .status()?;
    if !status.success() {
        return Err(format!("jeryu web export-schemas exited with {status}").into());
    }
    eprintln!(
        "schema export dir: {} ({})",
        config.out_dir.display(),
        config.source.label()
    );
    Ok(())
}

struct SchemaExportConfig {
    out_dir: PathBuf,
    source: SchemaOutputSource,
    cargo_bin: String,
}

enum SchemaOutputSource {
    Env,
    Default,
}

impl SchemaOutputSource {
    fn label(&self) -> &'static str {
        match self {
            Self::Env => "env:JERYU_SCHEMA_OUT_DIR",
            Self::Default => "default:schemas",
        }
    }
}

fn schema_export_config() -> SchemaExportConfig {
    let cargo_bin = match std::env::var("CARGO") {
        Ok(bin) if !bin.is_empty() => bin,
        _ => "cargo".to_string(),
    };
    match std::env::var_os("JERYU_SCHEMA_OUT_DIR") {
        Some(path) => SchemaExportConfig {
            out_dir: PathBuf::from(path),
            source: SchemaOutputSource::Env,
            cargo_bin,
        },
        None => SchemaExportConfig {
            out_dir: PathBuf::from("schemas"),
            source: SchemaOutputSource::Default,
            cargo_bin,
        },
    }
}
