//! Adapters for `jeryu ci {run,status,explain}`.

use std::io::Write;

use crate::cli::CiCommands;
use crate::client::{ClientResult, ForgeClient};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    json: bool,
    cmd: CiCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        CiCommands::Run {
            repo,
            git_ref,
            kind,
        } => {
            let run = client.ci_run(&repo, &git_ref, kind.into())?;
            render(
                out,
                json,
                &run,
                &format!(
                    "scheduled ci run {} for {}@{} ({} jobs)",
                    run.id, run.repo, run.git_ref, run.jobs
                ),
            )
        }
        CiCommands::Status { repo } => {
            let runs = client.ci_status(&repo)?;
            let human = runs
                .iter()
                .map(|r| format!("{} {}@{} [{:?}]", r.id, r.repo, r.git_ref, r.status))
                .collect::<Vec<_>>()
                .join("\n");
            render(out, json, &runs, &human)
        }
        CiCommands::Explain { run_id } => {
            let explanation = client.ci_explain(&run_id)?;
            let human = format!(
                "run {} blocked={}: {}",
                explanation.run_id,
                explanation.blocked,
                explanation.reasons.join("; ")
            );
            render(out, json, &explanation, &human)
        }
    }
}
