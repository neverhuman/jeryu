//! Binary entry point for the zero-evidence guard.
//!
//! Port of `scripts/zero-evidence-guard.py`: scan a workspace root (default
//! `.`) for blocked legacy/retired product markers. Prints one finding per
//! offending file to stderr and exits non-zero when any are present.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use jeryu_evidence::scan;

/// scan for blocked legacy-provider markers
#[derive(Parser, Debug)]
#[command(
    name = "jeryu-evidence",
    about = "scan for blocked legacy-provider markers"
)]
struct Cli {
    /// workspace root to scan
    #[arg(default_value = ".")]
    root: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let root = match std::fs::canonicalize(&cli.root) {
        Ok(root) => root,
        Err(source) => {
            eprintln!("error: cannot resolve {}: {source}", cli.root.display());
            return ExitCode::FAILURE;
        }
    };

    match scan(&root) {
        Ok(findings) if findings.is_empty() => ExitCode::SUCCESS,
        Ok(findings) => {
            for finding in &findings {
                eprintln!("{finding}");
            }
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
