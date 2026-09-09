#![forbid(unsafe_code)]

use jeryu_rustjet::{
    AffectedPlanner, ChangeSet, ExplainFormat, ExplainOutput, PlannerOptions, WorkspaceGraph,
};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("jeryu_rustjet: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<String, String> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Ok(help());
    }
    let command = args.remove(0);
    match command.as_str() {
        "explain" | "plan" => explain(args),
        other => Err(format!("unknown command `{other}`\n{}", help())),
    }
}

fn explain(args: Vec<String>) -> Result<String, String> {
    let mut workspace = PathBuf::from(".");
    let mut changed = Vec::new();
    let mut text = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--workspace" => {
                let Some(value) = iter.next() else {
                    return Err("--workspace requires a path".to_string());
                };
                workspace = PathBuf::from(value);
            }
            "--changed" => {
                let Some(value) = iter.next() else {
                    return Err("--changed requires a path".to_string());
                };
                changed.push(value);
            }
            "--text" => text = true,
            other => changed.push(other.to_string()),
        }
    }
    let graph = WorkspaceGraph::load(&workspace).map_err(|err| err.to_string())?;
    let changes = ChangeSet::from_strings(changed);
    let planner = AffectedPlanner::new(&graph);
    let plan = planner.plan(&changes, &PlannerOptions::default());
    Ok(ExplainOutput::render(
        &plan,
        if text {
            ExplainFormat::Text
        } else {
            ExplainFormat::Json
        },
    )
    .body)
}

fn help() -> String {
    "jeryu_rustjet commands:\n  explain --workspace <path> --changed <path> [--changed <path>] [--text]\n  plan    --workspace <path> --changed <path> [--text]".to_string()
}
