//! Owner: User settings subsystem
//! Proof: `cargo nextest run -p jeryu -- settings`
//! Invariants: Settings merges are deterministic and never silently discard explicit user configuration.
//!
//! Loads `~/.jeryu/settings.json`, creating it with defaults on first run.
//! All tunables that were previously env vars live here instead.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::OnceLock;

#[path = "settings_types.rs"]
mod settings_types;
pub use settings_types::*;

// ---------------------------------------------------------------------------
// Load / persist
// ---------------------------------------------------------------------------

/// Path to the settings file.
pub fn settings_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".jeryu")
        .join("settings.json")
}

/// Load settings from `~/.jeryu/settings.json`.
/// Creates the file with defaults if it does not exist.
/// Unknown keys are ignored for forward compatibility; missing keys use defaults for backward compatibility.
pub fn load() -> Result<Settings> {
    let path = settings_path();

    if !path.exists() {
        let dir = path.parent().expect("settings path has no parent");
        std::fs::create_dir_all(dir)?;
        let defaults = Settings::default();
        let json = serde_json::to_string_pretty(&defaults)?;
        std::fs::write(&path, json)?;
        tracing::info!(path = %path.display(), "created default settings.json");
        return Ok(defaults);
    }

    let raw = std::fs::read_to_string(&path)?;
    let settings: Settings = serde_json::from_str(&raw).map_err(|e| {
        let backup = path.with_file_name(format!(
            "settings.json.bad.{}",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        ));
        let _ = std::fs::copy(&path, &backup);
        anyhow::anyhow!(
            "settings.json parse error at {}: {} (backed up to {})",
            path.display(),
            e,
            backup.display()
        )
    })?;
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Process-wide singleton — populated once at startup
// ---------------------------------------------------------------------------

static SETTINGS: OnceLock<Settings> = OnceLock::new();

/// Initialize the process-wide settings. Call once at program entry.
/// Subsequent calls are ignored.
pub fn init() -> Result<()> {
    let s = load()?;
    let _ = SETTINGS.set(s);
    Ok(())
}

/// Access the process-wide settings. Panics if `init()` was not called.
pub fn get() -> &'static Settings {
    SETTINGS.get_or_init(|| load().expect("failed to load settings.json"))
}

#[path = "settings_support.rs"]
mod settings_support;
pub(crate) use settings_support::*;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod settings_tests;
