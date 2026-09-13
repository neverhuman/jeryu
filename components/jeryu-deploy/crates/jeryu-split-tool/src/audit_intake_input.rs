//! Private held-file reads for raw payloads, routing configuration and secret bytes.
use super::*;
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

pub(super) fn read_private(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .context("input needs a private parent directory")?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    let owner = fs::metadata("/proc/self")?.uid();
    ensure!(
        path.is_absolute()
            && path.canonicalize()? == path
            && parent_metadata.is_dir()
            && parent_metadata.uid() == owner
            && parent_metadata.mode() & 0o077 == 0,
        "intake input must have a physical owner-only parent"
    );
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file()
            && before.nlink() == 1
            && before.uid() == owner
            && before.mode() & 0o077 == 0
            && before.len() <= maximum as u64,
        "intake input must be a bounded owner-only ordinary file"
    );
    let mut bytes = Vec::new();
    file.by_ref()
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    let identity = |metadata: &fs::Metadata| {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.gid(),
            metadata.mode(),
            metadata.nlink(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    };
    let directory_identity = |metadata: &fs::Metadata| {
        (
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.gid(),
            metadata.mode(),
        )
    };
    ensure!(
        named.is_file()
            && identity(&before) == identity(&after)
            && identity(&before) == identity(&named)
            && bytes.len() as u64 == before.len()
            && path.canonicalize()? == path
            && directory_identity(&parent_metadata)
                == directory_identity(&fs::symlink_metadata(parent)?),
        "intake input changed during reception"
    );
    Ok(bytes)
}
