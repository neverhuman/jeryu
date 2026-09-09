//! Adapters for `jeryu proof {verify,explain}`.

use std::io::Write;

use crate::cli::ProofCommands;
use crate::client::{ClientResult, ForgeClient};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    json: bool,
    cmd: ProofCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        ProofCommands::Verify { changeset } => {
            let verdict = client.proof_verify(&changeset)?;
            let human = if verdict.admissible {
                format!("admissible (plan {})", verdict.plan_hash)
            } else {
                format!("blocked: {}", verdict.blockers.join("; "))
            };
            render(out, json, &verdict, &human)
        }
        ProofCommands::Explain { id } => {
            let verdict = client.proof_explain(&id)?;
            render(out, json, &verdict, &verdict.blockers.join("; "))
        }
    }
}
