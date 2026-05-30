//! Minimal CLI for local SignRail workflows.

use crate::artifact::Artifact;
use crate::checksum::sha256_file;
use crate::error::{Result, SignRailError};
use crate::sbom::SbomDocument;
use std::path::PathBuf;

/// Run the CLI using process arguments.
pub fn run_env() -> Result<String> {
    run_from(std::env::args().skip(1))
}

/// Run the CLI from an argument iterator.
pub fn run_from<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("checksum") => {
            let path = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput("usage: signrail checksum <path>".to_string())
            })?;
            Ok(format!("{}  {}", sha256_file(path)?, path))
        }
        Some("sbom") => {
            let version = args.get(1).ok_or_else(|| {
                SignRailError::InvalidInput(
                    "usage: signrail sbom <version> <artifact>...".to_string(),
                )
            })?;
            if args.len() < 3 {
                return Err(SignRailError::InvalidInput(
                    "usage: signrail sbom <version> <artifact>...".to_string(),
                ));
            }
            let mut artifacts = Vec::new();
            for path in args.iter().skip(2) {
                let path_buf = PathBuf::from(path);
                let name = path_buf
                    .file_name()
                    .and_then(|part| part.to_str())
                    .ok_or_else(|| {
                        SignRailError::InvalidInput(format!("invalid artifact path: {path}"))
                    })?
                    .to_string();
                artifacts.push(Artifact::from_file(
                    name,
                    path_buf,
                    "application/octet-stream",
                )?);
            }
            Ok(SbomDocument::from_artifacts(version, &artifacts, 0).to_json())
        }
        Some("help") | None => Ok(help()),
        Some(other) => Err(SignRailError::InvalidInput(format!(
            "unknown command {other}\n{}",
            help()
        ))),
    }
}

fn help() -> String {
    "signrail commands:\n  checksum <path>\n  sbom <version> <artifact>...".to_string()
}
