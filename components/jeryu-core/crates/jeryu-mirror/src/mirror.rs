use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::errors::{JeryuMirrorError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MirrorMode {
    CloneIfMissing,
    FetchOnly,
    PushOnly,
    FullSync,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MirrorSpec {
    pub name: String,
    pub source_url: String,
    pub destination_url: String,
    pub local_path: PathBuf,
    pub mode: MirrorMode,
    pub prune: bool,
    pub allowed_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitMirrorPlan {
    pub commands: Vec<Vec<String>>,
    pub notes: Vec<String>,
}

pub fn plan_git_mirror(spec: &MirrorSpec) -> GitMirrorPlan {
    let mut commands = Vec::new();
    let path = spec.local_path.to_string_lossy().to_string();
    match spec.mode {
        MirrorMode::CloneIfMissing => {
            commands.push(vec![
                "git".into(),
                "clone".into(),
                "--mirror".into(),
                spec.source_url.clone(),
                path.clone(),
            ]);
        }
        MirrorMode::FetchOnly => {
            commands.push(fetch_command(&path, spec.prune));
        }
        MirrorMode::PushOnly => {
            commands.push(push_command(
                &path,
                &spec.destination_url,
                spec.prune,
                &spec.allowed_refs,
            ));
        }
        MirrorMode::FullSync => {
            commands.push(vec![
                "git".into(),
                "clone".into(),
                "--mirror".into(),
                spec.source_url.clone(),
                path.clone(),
            ]);
            commands.push(fetch_command(&path, spec.prune));
            commands.push(push_command(
                &path,
                &spec.destination_url,
                spec.prune,
                &spec.allowed_refs,
            ));
        }
    }
    let notes = vec![
        "Commands are argument-vector plans; do not execute through a shell.".to_string(),
        "JeryuMirror never copies ambient credentials or SSH agents into restore commands."
            .to_string(),
    ];
    GitMirrorPlan { commands, notes }
}

pub fn execute_git_mirror_plan(plan: &GitMirrorPlan) -> Result<()> {
    for argv in &plan.commands {
        if argv.is_empty() {
            continue;
        }
        let status = Command::new(&argv[0]).args(&argv[1..]).status()?;
        if !status.success() {
            return Err(JeryuMirrorError::Git(format!(
                "command {:?} exited with {status}",
                argv
            )));
        }
    }
    Ok(())
}

pub fn repository_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().join("HEAD").exists() && path.as_ref().join("objects").exists()
}

fn fetch_command(path: &str, prune: bool) -> Vec<String> {
    let mut command = vec![
        "git".into(),
        "-C".into(),
        path.into(),
        "fetch".into(),
        "--mirror".into(),
    ];
    if prune {
        command.push("--prune".into());
    }
    command
}

fn push_command(
    path: &str,
    destination: &str,
    prune: bool,
    allowed_refs: &[String],
) -> Vec<String> {
    let mut command = vec![
        "git".into(),
        "-C".into(),
        path.into(),
        "push".into(),
        "--mirror".into(),
        destination.into(),
    ];
    if prune {
        command.push("--prune".into());
    }
    if !allowed_refs.is_empty() {
        command.push(format!(
            "--push-option=jeryu-allowed-refs={}",
            allowed_refs.join(",")
        ));
    }
    command
}
