//! Parsing of a job's `steps:` block into IR [`Step`]s, plus matrix expansion
//! of step commands.

use std::collections::BTreeMap;

use jeryu_ci_ir::{Step, trim_quotes};

use crate::error::CompileError;
use crate::lexer::{SourceLine, collect_block_scalar, is_block_scalar};

#[derive(Default)]
pub(crate) struct StepBuilder {
    pub(crate) name: Option<String>,
    pub(crate) run: Option<String>,
    pub(crate) uses: Option<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) working_directory: Option<String>,
}

pub(crate) fn parse_steps(origin: &str, lines: &[SourceLine]) -> Result<Vec<Step>, CompileError> {
    let mut builders = Vec::new();
    let mut current: Option<StepBuilder> = None;
    let mut in_env = false;
    let mut env_indent = 0;

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.text.starts_with("- ") {
            if let Some(builder) = current.take() {
                builders.push(builder);
            }
            current = Some(StepBuilder::default());
            in_env = false;
            let rest = line.text.trim_start_matches("- ").trim();
            if let Some((key, value)) = rest.split_once(':') {
                let value = value.trim();
                if key.trim() == "run" && is_block_scalar(value) {
                    let (body, next) = collect_block_scalar(lines, i + 1, line.indent, value);
                    if let Some(builder) = current.as_mut() {
                        builder.run = Some(body);
                    }
                    i = next;
                    continue;
                }
                apply_step_attr(current.as_mut(), key.trim(), value);
            }
            i += 1;
            continue;
        }
        if current.is_none() {
            i += 1;
            continue;
        }
        if in_env
            && line.indent > env_indent
            && let Some((key, value)) = line.text.split_once(':')
            && let Some(builder) = current.as_mut()
        {
            builder.env.insert(
                key.trim().to_string(),
                trim_quotes(value.trim()).to_string(),
            );
            i += 1;
            continue;
        }
        if line.text == "env:" {
            in_env = true;
            env_indent = line.indent;
            i += 1;
            continue;
        }
        in_env = false;
        if let Some((key, value)) = line.text.split_once(':') {
            let value = value.trim();
            if key.trim() == "run" && is_block_scalar(value) {
                let (body, next) = collect_block_scalar(lines, i + 1, line.indent, value);
                if let Some(builder) = current.as_mut() {
                    builder.run = Some(body);
                }
                i = next;
                continue;
            }
            apply_step_attr(current.as_mut(), key.trim(), value);
        }
        i += 1;
    }
    if let Some(builder) = current.take() {
        builders.push(builder);
    }

    builders
        .into_iter()
        .enumerate()
        .map(|(idx, builder)| step_from_builder(origin, idx, builder))
        .collect()
}

fn apply_step_attr(builder: Option<&mut StepBuilder>, key: &str, value: &str) {
    if let Some(builder) = builder {
        match key {
            "name" => builder.name = Some(trim_quotes(value).to_string()),
            "run" => builder.run = Some(trim_quotes(value).to_string()),
            "uses" => builder.uses = Some(trim_quotes(value).to_string()),
            "working-directory" => builder.working_directory = Some(trim_quotes(value).to_string()),
            _ => {}
        }
    }
}

fn step_from_builder(origin: &str, idx: usize, builder: StepBuilder) -> Result<Step, CompileError> {
    if builder.run.as_ref().is_none_or(|run| run.trim().is_empty())
        && builder
            .uses
            .as_ref()
            .is_none_or(|uses| uses.trim().is_empty())
    {
        return Err(CompileError::MissingSteps(origin.to_string()));
    }
    let id = format!("step_{idx:02}");
    let name = builder.name.clone().unwrap_or_else(|| {
        builder
            .run
            .clone()
            .or(builder.uses.clone())
            .unwrap_or_else(|| "step".to_string())
    });
    Ok(Step {
        id,
        name,
        command: builder.run,
        uses: builder.uses,
        env: builder.env,
        working_directory: builder.working_directory,
    })
}

pub(crate) fn steps_with_matrix(
    steps: &[Step],
    matrix: &BTreeMap<String, String>,
    job_id: &str,
) -> Vec<Step> {
    steps
        .iter()
        .enumerate()
        .map(|(idx, step)| {
            let mut next = step.clone();
            next.id = format!("{job_id}_step_{idx:02}");
            for (key, value) in matrix {
                let needle = format!("${{{{ matrix.{key} }}}}");
                if let Some(command) = next.command.as_mut() {
                    *command = command.replace(&needle, value);
                }
                if let Some(uses) = next.uses.as_mut() {
                    *uses = uses.replace(&needle, value);
                }
                next.name = next.name.replace(&needle, value);
            }
            next
        })
        .collect()
}
