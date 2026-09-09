//! Adapters for `jeryu runner {list,enroll,drain,rotate}`.

use std::io::Write;

use crate::cli::RunnerCommands;
use crate::client::{ClientResult, ForgeClient};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    json: bool,
    cmd: RunnerCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        RunnerCommands::List => {
            let runners = client.runner_list()?;
            let human = runners
                .iter()
                .map(|r| format!("{} [{:?}] accepting={}", r.id, r.executor, r.accepting))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &runners, &human)
        }
        RunnerCommands::Enroll { node, executor } => {
            let runner = client.runner_enroll(&node, executor.into())?;
            render(
                out,
                json,
                &runner,
                &format!("enrolled runner {} [{:?}]", runner.id, runner.executor),
            )
        }
        RunnerCommands::Drain { id } => {
            let runner = client.runner_drain(&id)?;
            render(
                out,
                json,
                &runner,
                &format!("draining runner {}", runner.id),
            )
        }
        RunnerCommands::Rotate { id } => {
            let cred = client.runner_rotate(&id)?;
            render(
                out,
                json,
                &serde_json::json!({ "runner": id, "credential": cred }),
                &format!("rotated credential for runner {id}"),
            )
        }
    }
}
