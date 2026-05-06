//! Owner: Settings repair/reset commands
//! Proof: `cargo test -p jeryu -- settings`
//! Invariants: Corrupt settings are repaired from backups or reset explicitly; nothing silently falls back.

use anyhow::{Result, bail};
use std::fs;
use std::path::PathBuf;

pub(crate) async fn execute_settings_commands(cmd: crate::cli::SettingsCommands) -> Result<()> {
    match cmd {
        crate::cli::SettingsCommands::Repair => repair_settings().await,
        crate::cli::SettingsCommands::Reset { force } => reset_settings(force).await,
    }
}

async fn repair_settings() -> Result<()> {
    match jeryu::settings::load() {
        Ok(_) => {
            println!("✅ settings.json is valid.");
            Ok(())
        }
        Err(err) => {
            let path = jeryu::settings::settings_path();
            let backup = if let Some(backup) = newest_backup(&path) {
                backup
            } else {
                return Err(anyhow::anyhow!(
                    "settings.json is corrupt and no backup was found: {}",
                    err
                ));
            };
            fs::copy(&backup, &path)?;
            let _ = jeryu::settings::load()?;
            println!("✅ restored settings.json from backup {}", backup.display());
            Ok(())
        }
    }
}

async fn reset_settings(force: bool) -> Result<()> {
    let path = jeryu::settings::settings_path();
    if path.exists() && !force {
        bail!("refusing to reset existing settings without --force");
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let defaults = jeryu::settings::Settings::default();
    fs::write(&path, serde_json::to_string_pretty(&defaults)?)?;
    println!("✅ reset settings.json to defaults");
    Ok(())
}

fn newest_backup(path: &PathBuf) -> Option<PathBuf> {
    let dir = path.parent()?;
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|entry| {
            entry
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("settings.json.bad."))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    entries.pop()
}
