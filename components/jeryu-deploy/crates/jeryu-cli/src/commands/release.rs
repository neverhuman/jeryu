//! Adapter for `jeryu release`.

use std::io::Write;

use crate::client::{ClientResult, ForgeClient};
use crate::commands::render;

pub(crate) fn run(
    client: &dyn ForgeClient,
    json: bool,
    version: &str,
    out: &mut dyn Write,
) -> ClientResult<()> {
    let record = client.release_ready(version)?;
    let human = format!(
        "release {} ready={} witness={}",
        record.version, record.ready, record.witness
    );
    render(out, json, &record, &human)
}
