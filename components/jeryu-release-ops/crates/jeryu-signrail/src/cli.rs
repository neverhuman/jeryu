//! Minimal CLI for local SignRail workflows.

mod sign;
mod sign_args;
mod support;
mod verify;
mod verify_payload;

use crate::checksum::sha256_file;
use crate::error::{Result, SignRailError};
use crate::sbom::SbomDocument;
use sign::sign_release;
use support::{artifacts_from_paths, help};
use verify::verify_release;

/// Run the CLI using process arguments.
pub fn run_env() -> Result<String> {
    run_from_with_env(std::env::args().skip(1), |key| std::env::var(key).ok())
}

/// Run the CLI from an argument iterator.
pub fn run_from<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    run_from_with_env(args, |key| std::env::var(key).ok())
}

/// Run the CLI with an injected environment lookup.
pub fn run_from_with_env<I, S, F>(args: I, env: F) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    F: Fn(&str) -> Option<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("checksum") => {
            let path = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput("usage: jeryu_signrail checksum <path>".to_string())
            })?;
            Ok(format!("{}  {}", sha256_file(path)?, path))
        }
        Some("sbom") => {
            let version = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput(
                    "usage: jeryu_signrail sbom <version> <artifact>...".to_string(),
                )
            })?;
            if args.len() < 3 {
                return Err(SignRailError::InvalidInput(
                    "usage: jeryu_signrail sbom <version> <artifact>...".to_string(),
                ));
            }
            let artifacts = artifacts_from_paths(args.iter().skip(2))?;
            Ok(SbomDocument::from_artifacts(version, &artifacts, 0).to_json())
        }
        Some("sign-release") => sign_release(&args[1..], &env),
        Some("verify-release") => verify_release(&args[1..]),
        Some("help") | None => Ok(help()),
        Some(other) => Err(SignRailError::InvalidInput(format!(
            "unknown command {other}\n{}",
            help()
        ))),
    }
}
