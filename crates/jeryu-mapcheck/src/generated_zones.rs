//! Port of `scripts/check-generated-zones.py`: required generated zones + generators.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::render::{py_repr, py_repr_opt};
use crate::report::GateReport;

#[derive(Debug, Deserialize)]
struct Zone {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    generator: Option<String>,
    #[serde(default)]
    manual_edits: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GeneratedZones {
    #[serde(default)]
    zones: Vec<Zone>,
}

/// Required generated zones and their expected generators, in the order the
/// Python dict declared them.
const REQUIRED_ZONES: [(&str, &str); 3] = [
    ("docs/generated/**", "scripts/render-policy-docs.sh"),
    ("receipts/generated/**", "jeryu-cache-service"),
    (
        "contracts/generated/**",
        "cargo run -p jeryu-readmodel --bin export_contracts",
    ),
];

/// Port of `scripts/check-generated-zones.py`.
///
/// Validates that the generated-zones manifest declares each required zone with
/// the expected generator and `manual_edits = false`.
///
/// # Errors
/// Returns an error if the manifest cannot be read or parsed as TOML.
pub fn generated_zones(zones_toml: &Path) -> Result<GateReport> {
    let raw = std::fs::read_to_string(zones_toml)
        .with_context(|| format!("reading {}", zones_toml.display()))?;
    let config: GeneratedZones = toml::from_str(&raw)
        .with_context(|| format!("parsing {} as TOML", zones_toml.display()))?;

    let mut missing = Vec::new();
    for (zone_path, generator) in REQUIRED_ZONES {
        let zone = config
            .zones
            .iter()
            .find(|z| z.path.as_deref() == Some(zone_path));
        match zone {
            None => missing.push(format!("missing generated zone: {zone_path}")),
            Some(zone) => {
                if zone.generator.as_deref() != Some(generator) {
                    missing.push(format!(
                        "generated zone {zone_path} has generator {}, expected {}",
                        py_repr_opt(zone.generator.as_deref()),
                        py_repr(generator)
                    ));
                } else if zone.manual_edits != Some(false) {
                    missing.push(format!(
                        "generated zone {zone_path} must set manual_edits = false"
                    ));
                }
            }
        }
    }

    if missing.is_empty() {
        Ok(GateReport::pass("generated zones ok"))
    } else {
        Ok(GateReport::fail(missing))
    }
}
