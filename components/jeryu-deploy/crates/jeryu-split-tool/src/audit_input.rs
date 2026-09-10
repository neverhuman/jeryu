//! Bounded file custody before any auditor or receipt verifier is started.
use anyhow::{Result, ensure};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

const REPORT_LIMIT: u64 = 32 * 1024 * 1024;
const BINARY_LIMIT: u64 = 128 * 1024 * 1024;

fn regular(file: &fs::File, limit: u64) -> Result<fs::Metadata> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.nlink() == 1 && metadata.len() <= limit,
        "audit input must be a bounded single-link regular file"
    );
    Ok(metadata)
}

fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}

fn read_held(file: &fs::File, limit: u64) -> Result<Vec<u8>> {
    let before = regular(file, limit)?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        identity(&before) == identity(&regular(file, limit)?) && bytes.len() as u64 == before.len(),
        "audit input changed while being read"
    );
    Ok(bytes)
}

fn open(path: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?)
}

pub(super) fn read_report(path: &Path) -> Result<Vec<u8>> {
    let file = open(path)?;
    let bytes = read_held(&file, REPORT_LIMIT)?;
    let named = fs::symlink_metadata(path)?;
    ensure!(
        named.is_file() && named.nlink() == 1 && identity(&named) == identity(&file.metadata()?),
        "audit input path changed"
    );
    Ok(bytes)
}

pub(super) fn hold_binary(path: &Path) -> Result<fs::File> {
    let file = open(path)?;
    regular(&file, BINARY_LIMIT)?;
    Ok(file)
}

// Only the parent-generated /proc/<parent>/fd/<held> path reaches this helper.
// Following that descriptor is intentional; submitted paths use hold_binary.
pub(super) fn hash_binary(held_path: &Path) -> Result<String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(held_path)?;
    Ok(crate::audit_evidence::hash(&read_held(
        &file,
        BINARY_LIMIT,
    )?))
}
