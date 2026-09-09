//! Adapter for `jeryu cache self-test`.

use std::io::Write;

use crate::cli::CacheCommands;
use crate::client::{ClientResult, ForgeClient};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    json: bool,
    cmd: CacheCommands,
    out: &mut dyn Write,
) -> ClientResult<()> {
    match cmd {
        CacheCommands::SelfTest => {
            let report = client.cache_self_test()?;
            let human = format!(
                "cache self-test {}: probes={} false_hits={}",
                if report.passed { "passed" } else { "FAILED" },
                report.probes,
                report.false_hits
            );
            render(out, json, &report, &human)
        }
    }
}
