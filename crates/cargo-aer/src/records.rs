use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use walkdir::WalkDir;

use super::*;

pub fn init_records(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let records_dir = workspace_root.join("aer-records");
    fs::create_dir_all(&records_dir)
        .with_context(|| format!("failed to create {}", records_dir.display()))?;
    let readme = records_dir.join("README.md");
    if !readme.exists() {
        fs::write(
            &readme,
            "# Agent Exception Records\n\nEach YAML file captures one explicit break from a strict default.\n",
        )
        .with_context(|| format!("failed to write {}", readme.display()))?;
    }
    let example = records_dir.join("EXAMPLE.yaml");
    if !example.exists() {
        fs::write(
            &example,
            r#"id: aer.example.mega-file-parser
class_id: mega-file
rule: "Files should stay below the house budget unless locality would be harmed."
exception: "Parser table remains in one file to preserve traceability."
reason: "Splitting the grammar table would make cross-rule debugging harder during a protocol migration."
risk: medium
owner: parser-team
doc_links:
  - https://doc.rust-lang.org/book/ch13-04-performance.html
sunset_condition: "Remove when the grammar format is stabilized and table generation lands."
"#,
        )
        .with_context(|| format!("failed to write {}", example.display()))?;
    }
    Ok(vec![readme, example])
}

pub fn incomplete_records(workspace_root: &Path) -> Result<Vec<Finding>> {
    let records_dir = workspace_root.join("aer-records");
    let mut findings = Vec::new();
    if !records_dir.exists() {
        return Ok(findings);
    }
    for entry in WalkDir::new(&records_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if !path.is_file()
            || !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("yaml" | "yml")
            )
        {
            continue;
        }
        let record = parse_record(path)?;
        if record.owner.trim().is_empty() {
            findings.push(Finding {
                class_id: "incomplete-aer".to_string(),
                severity: "warning".to_string(),
                confidence: 0.95,
                path: display_relative(workspace_root, path),
                summary: "AER is missing an owner".to_string(),
                suggested_fix: "Set owner so the exception has clear stewardship.".to_string(),
                existing_exception: Some(record.id.clone()),
            });
        }
        if record.sunset_condition.trim().is_empty() {
            findings.push(Finding {
                class_id: "incomplete-aer".to_string(),
                severity: "warning".to_string(),
                confidence: 0.95,
                path: display_relative(workspace_root, path),
                summary: "AER is missing a sunset condition".to_string(),
                suggested_fix: "Add a concrete reevaluation condition or review date.".to_string(),
                existing_exception: Some(record.id.clone()),
            });
        }
    }
    Ok(findings)
}

fn parse_record(path: &Path) -> Result<AerRecord> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}
