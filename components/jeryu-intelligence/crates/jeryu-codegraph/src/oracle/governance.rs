use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::types::GeneratedZoneHit;
use crate::error::{CodeGraphError, Result};

const GOVERNANCE_FILES: [(&str, &str); 5] = [
    ("AGENTS.md", "agents"),
    ("agent/owner-map.json", "owner_map"),
    ("agent/test-map.json", "test_map"),
    ("agent/generated-zones.toml", "generated_zones"),
    ("agent/proof-lanes.toml", "proof_lanes"),
];

#[derive(Debug, Default)]
pub(super) struct GovernanceMetadata {
    owner_rules: Vec<OwnerRule>,
    test_rules: Vec<TestRule>,
    generated_zones: Vec<GeneratedZoneRule>,
    pub(super) proof_lanes: BTreeMap<String, ProofLaneRule>,
    pub(super) loaded_files: Vec<LoadedGovernanceFile>,
    pub(super) repo_files: Vec<String>,
}

impl GovernanceMetadata {
    pub(super) fn load(root: &Path) -> Result<Self> {
        let mut metadata = Self {
            repo_files: collect_repo_files(root)?,
            ..Self::default()
        };

        for (path, kind) in GOVERNANCE_FILES {
            let full = root.join(path);
            if !full.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&full).map_err(|source| CodeGraphError::Index {
                path: full.display().to_string(),
                source,
            })?;
            metadata.loaded_files.push(LoadedGovernanceFile {
                path: path.to_string(),
                kind: kind.to_string(),
                digest: content_digest(&text),
            });
            match kind {
                "owner_map" => {
                    let parsed: OwnerMapFile =
                        serde_json::from_str(&text).map_err(|err| CodeGraphError::Governance {
                            path: path.to_string(),
                            message: err.to_string(),
                        })?;
                    metadata.owner_rules = parsed
                        .owners
                        .into_iter()
                        .map(|(path, owner)| OwnerRule { path, owner })
                        .collect();
                    metadata.owner_rules.sort_by(|a, b| a.path.cmp(&b.path));
                }
                "test_map" => {
                    let parsed: TestMapFile =
                        serde_json::from_str(&text).map_err(|err| CodeGraphError::Governance {
                            path: path.to_string(),
                            message: err.to_string(),
                        })?;
                    metadata.test_rules = parsed
                        .tests
                        .into_iter()
                        .map(|(path, rule)| TestRule {
                            path,
                            command: rule.command,
                            purpose: rule.purpose,
                            lane: rule.lane,
                        })
                        .collect();
                    metadata.test_rules.sort_by(|a, b| a.path.cmp(&b.path));
                }
                "generated_zones" => {
                    let parsed: GeneratedZonesFile =
                        toml::from_str(&text).map_err(|err| CodeGraphError::Governance {
                            path: path.to_string(),
                            message: err.to_string(),
                        })?;
                    metadata.generated_zones = parsed.zones;
                    metadata.generated_zones.sort_by(|a, b| a.path.cmp(&b.path));
                }
                "proof_lanes" => {
                    let parsed: ProofLanesFile =
                        toml::from_str(&text).map_err(|err| CodeGraphError::Governance {
                            path: path.to_string(),
                            message: err.to_string(),
                        })?;
                    metadata.proof_lanes = parsed.lanes;
                }
                _ => {}
            }
        }
        Ok(metadata)
    }

    pub(super) fn owner_for_path(&self, path: &str) -> Option<String> {
        self.owner_rules
            .iter()
            .filter(|rule| rule_matches(&rule.path, path))
            .max_by_key(|rule| rule.path.len())
            .map(|rule| rule.owner.clone())
    }

    pub(super) fn test_for_path(&self, path: &str) -> Option<&TestRule> {
        self.test_rules
            .iter()
            .filter(|rule| rule_matches(&rule.path, path))
            .max_by_key(|rule| rule.path.len())
    }

    pub(super) fn generated_zone_for_path(&self, path: &str) -> Option<GeneratedZoneHit> {
        self.generated_zones
            .iter()
            .filter(|zone| rule_matches(&zone.path, path))
            .max_by_key(|zone| zone.path.len())
            .map(|zone| GeneratedZoneHit {
                path: zone.path.clone(),
                generator: zone.generator.clone(),
                manual_edits: zone.manual_edits,
            })
    }
}

#[derive(Debug, Deserialize)]
struct OwnerMapFile {
    owners: BTreeMap<String, String>,
}

#[derive(Debug)]
struct OwnerRule {
    path: String,
    owner: String,
}

#[derive(Debug, Deserialize)]
struct TestMapFile {
    tests: BTreeMap<String, TestMapEntry>,
}

#[derive(Debug, Deserialize)]
struct TestMapEntry {
    command: String,
    purpose: String,
    lane: String,
}

#[derive(Debug)]
pub(super) struct TestRule {
    path: String,
    pub(super) command: String,
    pub(super) purpose: String,
    pub(super) lane: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedZonesFile {
    zones: Vec<GeneratedZoneRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct GeneratedZoneRule {
    path: String,
    generator: String,
    manual_edits: bool,
}

#[derive(Debug, Deserialize)]
struct ProofLanesFile {
    lanes: BTreeMap<String, ProofLaneRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ProofLaneRule {
    #[serde(default)]
    pub(super) required: Vec<String>,
    #[serde(default)]
    pub(super) blocks_merge: bool,
}

#[derive(Debug)]
pub(super) struct LoadedGovernanceFile {
    pub(super) path: String,
    pub(super) kind: String,
    pub(super) digest: String,
}

fn collect_repo_files(root: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| CodeGraphError::Index {
            path: dir.display().to_string(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| CodeGraphError::Index {
                path: dir.display().to_string(),
                source,
            })?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), ".git" | "target" | "node_modules" | ".jeryu") {
                continue;
            }
            let file_type = entry.file_type().map_err(|source| CodeGraphError::Index {
                path: path.display().to_string(),
                source,
            })?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn rule_matches(rule: &str, path: &str) -> bool {
    if let Some(prefix) = rule.strip_suffix("**") {
        return path.starts_with(prefix);
    }
    if rule.ends_with('/') {
        return path.starts_with(rule);
    }
    path == rule || path.starts_with(&format!("{rule}/"))
}

fn content_digest(text: &str) -> String {
    let sum = text
        .as_bytes()
        .iter()
        .fold(0_u64, |acc, byte| acc.wrapping_add(u64::from(*byte)));
    format!("len:{}:sum:{sum:016x}", text.len())
}
