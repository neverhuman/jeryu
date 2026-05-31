//! Small YAML `run:` extractor for CI workflow drift checks.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

pub(super) fn workflow_files(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let dir = root.join(".github/workflows");
    if !dir.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(&dir).with_context(|| "read .github/workflows")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if ext == "yml" || ext == "yaml" {
            files.push(relative_path(root, &path));
        }
    }
    files.sort();
    Ok(files)
}

pub(super) fn workflow_declares_job(text: &str, job: &str) -> bool {
    let wanted = format!("{job}:");
    text.lines().any(|line| line.trim() == wanted)
}

pub(super) fn extract_run_commands(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut commands = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.starts_with("run:") {
            index += 1;
            continue;
        }

        let indent = leading_indent(line);
        let rest = trimmed["run:".len()..].trim_start();
        if is_block_scalar(rest) {
            index += 1;
            let mut block = Vec::new();
            while index < lines.len() {
                let next = lines[index];
                if next.trim().is_empty() {
                    block.push(next);
                    index += 1;
                    continue;
                }
                if leading_indent(next) <= indent {
                    break;
                }
                block.push(next);
                index += 1;
            }
            if let Some(command) = normalize_block(&block) {
                commands.push(command);
            }
        } else {
            let command = normalize_command(rest);
            if !command.is_empty() {
                commands.push(command);
            }
            index += 1;
        }
    }
    commands
}

pub(super) fn normalize_command(command: &str) -> String {
    command
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_block_scalar(rest: &str) -> bool {
    matches!(rest, "|" | "|-" | "|+" | ">" | ">-" | ">+")
}

fn normalize_block(lines: &[&str]) -> Option<String> {
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| leading_indent(line))
        .min()
        .unwrap_or(0);
    let normalized = lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                *line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let command = normalize_command(&normalized);
    if command.is_empty() {
        None
    } else {
        Some(command)
    }
}

fn leading_indent(line: &str) -> usize {
    line.as_bytes()
        .iter()
        .take_while(|byte| **byte == b' ' || **byte == b'\t')
        .count()
}
