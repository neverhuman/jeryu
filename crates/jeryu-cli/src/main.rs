//! The `jeryu` operator/agent CLI binary.

use std::io::{self, Write};
use std::process::ExitCode;

use clap::Parser;
use jeryu_cli::{Cli, InMemoryClient, dispatch};

fn main() -> ExitCode {
    let cli = Cli::parse();
    // The binary runs against the in-memory client; swapping in an
    // `jeryu-api`/`jeryu-core`-backed client uses the identical dispatch seam.
    let client = InMemoryClient::new();

    let stdout = io::stdout();
    let stderr = io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();

    let code = dispatch(cli, &client, &mut out, &mut err);
    out.flush().ok();
    err.flush().ok();

    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
