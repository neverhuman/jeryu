//! The `jeryu` operator/agent CLI binary.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use jeryu_cli::{Cli, StubClient, dispatch};

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Until the `jeryu-api`/`jeryu-core` handles are wired, the binary runs
    // against the in-memory stub client. The dispatch seam is identical.
    let client = StubClient::new();

    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();

    let code = dispatch(cli, &client, &mut out, &mut err);
    out.flush().ok();
    err.flush().ok();

    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
