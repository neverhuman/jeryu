//! Artifact-support evidence writer for deploy release CI.

use std::fs;
use std::fs::File;
use std::io::{BufReader, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Evidence(Box<EvidenceArgs>),
    Pubkey(PubkeyArgs),
}

#[derive(Debug, Parser)]
struct EvidenceArgs {
    #[arg(long)]
    evidence_dir: PathBuf,
    #[arg(long)]
    binary: PathBuf,
    #[arg(long)]
    web_dist: PathBuf,
    #[arg(long)]
    route_probe: PathBuf,
    #[arg(long)]
    commit: String,
    #[arg(long)]
    tree: String,
    #[arg(long)]
    repo: String,
    #[arg(long)]
    version: String,
    #[arg(long)]
    manifest_sha: String,
    #[arg(long)]
    split_lock_sha: String,
    #[arg(long)]
    cargo_lock_sha: String,
    #[arg(long)]
    toolchain_sha: String,
    #[arg(long)]
    runner_policy_sha: String,
}

#[derive(Debug, Parser)]
struct PubkeyArgs {
    #[arg(long)]
    summary: PathBuf,
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Serialize)]
struct BinaryEvidence {
    schema_version: &'static str,
    commit: String,
    tree: String,
    path: String,
    present: bool,
    sha256: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct WebAsset {
    path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Serialize)]
struct WebAssetsEvidence {
    schema_version: &'static str,
    dist: String,
    asset_count: usize,
    assets: Vec<WebAsset>,
}

#[derive(Debug, Serialize)]
struct SplitReleaseEvidence {
    schema_version: &'static str,
    repo: String,
    version: String,
    commit: String,
    tree: String,
    manifest_sha256: Option<String>,
    split_lock_sha256: Option<String>,
    cargo_lock_sha256: Option<String>,
    toolchain_sha256: String,
    runner_policy_sha256: Option<String>,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Evidence(args) => write_evidence(*args),
        Command::Pubkey(args) => write_pubkey(args),
    }
}

fn write_evidence(args: EvidenceArgs) -> Result<()> {
    fs::create_dir_all(&args.evidence_dir).context("create evidence directory")?;
    write_json(
        &args.evidence_dir.join("binary.json"),
        &binary_evidence(&args)?,
    )?;
    write_json(
        &args.evidence_dir.join("web-assets.json"),
        &web_assets_evidence(&args.web_dist)?,
    )?;
    write_route_probe(
        &args.route_probe,
        &args.evidence_dir.join("route-probe.json"),
    )?;
    write_json(
        &args.evidence_dir.join("split-release.json"),
        &SplitReleaseEvidence {
            schema_version: "jeryu.split.artifact-support/v2",
            repo: args.repo,
            version: args.version,
            commit: args.commit,
            tree: args.tree,
            manifest_sha256: none_if_empty(args.manifest_sha),
            split_lock_sha256: none_if_empty(args.split_lock_sha),
            cargo_lock_sha256: none_if_empty(args.cargo_lock_sha),
            toolchain_sha256: args.toolchain_sha,
            runner_policy_sha256: none_if_empty(args.runner_policy_sha),
        },
    )?;
    Ok(())
}

fn binary_evidence(args: &EvidenceArgs) -> Result<BinaryEvidence> {
    let metadata = fs::metadata(&args.binary).ok();
    let is_file = metadata.as_ref().is_some_and(fs::Metadata::is_file);
    #[cfg(unix)]
    let executable = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.permissions().mode() & 0o111 != 0);
    #[cfg(not(unix))]
    let executable = is_file;
    let (sha256, size_bytes) = if is_file {
        (
            Some(sha256_file(&args.binary)?),
            Some(metadata.as_ref().expect("metadata exists").len()),
        )
    } else {
        (None, None)
    };
    Ok(BinaryEvidence {
        schema_version: "jeryu.artifact-support.binary/v1",
        commit: args.commit.clone(),
        tree: args.tree.clone(),
        path: args.binary.to_string_lossy().to_string(),
        present: is_file && executable,
        sha256,
        size_bytes,
    })
}

fn web_assets_evidence(dist: &Path) -> Result<WebAssetsEvidence> {
    let mut files = Vec::new();
    if dist.is_dir() {
        collect_files(dist, dist, &mut files)?;
    }
    files.sort();
    let mut assets = Vec::with_capacity(files.len());
    for path in files {
        let metadata = fs::metadata(&path)
            .with_context(|| format!("read web asset metadata: {}", path.display()))?;
        assets.push(WebAsset {
            path: relative_slash_path(dist, &path)?,
            sha256: sha256_file(&path)?,
            size_bytes: metadata.len(),
        });
    }
    Ok(WebAssetsEvidence {
        schema_version: "jeryu.web-assets.manifest/v1",
        dist: dist.to_string_lossy().to_string(),
        asset_count: assets.len(),
        assets,
    })
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read directory: {}", dir.display()))? {
        let entry = entry.context("read directory entry")?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("read metadata: {}", path.display()))?;
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() {
            let _ = path
                .strip_prefix(root)
                .with_context(|| format!("asset outside root: {}", path.display()))?;
            files.push(path);
        }
    }
    Ok(())
}

fn relative_slash_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("asset outside web dist: {}", path.display()))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            other => bail!("unsupported web asset path component: {other:?}"),
        }
    }
    Ok(parts.join("/"))
}

fn write_route_probe(route_probe: &Path, out: &Path) -> Result<()> {
    if route_probe.is_file() {
        fs::copy(route_probe, out).with_context(|| {
            format!(
                "copy route probe receipt {} -> {}",
                route_probe.display(),
                out.display()
            )
        })?;
        return Ok(());
    }
    write_json(
        out,
        &json!({
            "schema_version": "jeryu.route-probe/v1",
            "status": "not_run",
            "reason": "route probe receipt not present",
        }),
    )
}

fn write_pubkey(args: PubkeyArgs) -> Result<()> {
    let summary: Value = serde_json::from_reader(
        File::open(&args.summary)
            .with_context(|| format!("open SignRail summary: {}", args.summary.display()))?,
    )
    .context("parse SignRail summary")?;
    let pubkey = summary
        .get("signer_public_key_hex")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .context("summary.json missing signer_public_key_hex")?;
    fs::write(&args.out, format!("{pubkey}\n"))
        .with_context(|| format!("write pubkey file: {}", args.out.display()))
}

fn sha256_file(path: &Path) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("open file for sha256: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let len = reader
            .read(&mut buffer)
            .with_context(|| format!("read file for sha256: {}", path.display()))?;
        if len == 0 {
            break;
        }
        hasher.update(&buffer[..len]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn write_json<T: Serialize>(path: &Path, payload: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(payload).context("serialize JSON evidence")?;
    fs::write(path, format!("{text}\n"))
        .with_context(|| format!("write JSON evidence: {}", path.display()))
}

fn none_if_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}
