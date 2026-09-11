//! Custody checks for newly created disposable public-source clones only.
use anyhow::{Result, ensure};
use std::{fs, os::unix::fs::MetadataExt, path::Path};

pub(super) fn owned_directory(path: &Path) -> Result<(u64, u64, u32)> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && path.canonicalize()? == path
            && metadata.is_dir()
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.mode() & 0o077 == 0,
        "public source cache must be physical and owner-only"
    );
    Ok((metadata.dev(), metadata.ino(), metadata.uid()))
}

pub(super) fn unchanged_directory(path: &Path, identity: (u64, u64, u32)) -> Result<()> {
    ensure!(
        owned_directory(path)? == identity,
        "retaining replaced public source unit"
    );
    Ok(())
}

pub(super) fn inspect(root: &Path, max_bytes: u64, max_entries: usize) -> Result<()> {
    let metadata = fs::symlink_metadata(root)?;
    let mut pending = vec![root.to_owned()];
    let mut bytes = 0u64;
    let mut entries = 0usize;
    while let Some(path) = pending.pop() {
        let current = fs::symlink_metadata(&path)?;
        ensure!(
            current.uid() == metadata.uid() && current.dev() == metadata.dev(),
            "public source custody changed"
        );
        entries = entries
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("public source size overflow"))?;
        bytes = bytes
            .checked_add(current.len())
            .ok_or_else(|| anyhow::anyhow!("public source size overflow"))?;
        ensure!(
            entries <= max_entries && bytes <= max_bytes,
            "public source exceeded cache admission bounds"
        );
        if current.is_dir() {
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        } else if current.file_type().is_symlink() {
            ensure!(
                path.canonicalize()?.starts_with(root),
                "public source contains a dangling or external symbolic link"
            );
        } else {
            ensure!(
                current.is_file() && current.nlink() == 1,
                "public source has special or shared files"
            );
        }
    }
    Ok(())
}

fn mount_path(encoded: &str) -> Result<std::path::PathBuf> {
    let mut bytes = Vec::new();
    let mut rest = encoded.as_bytes();
    while !rest.is_empty() {
        if rest[0] == b'\\' {
            ensure!(
                rest.len() >= 4 && rest[1..4].iter().all(|b| (b'0'..=b'7').contains(b)),
                "invalid mount path encoding"
            );
            let octal = (rest[1] - b'0') as u16 * 64
                + (rest[2] - b'0') as u16 * 8
                + (rest[3] - b'0') as u16;
            ensure!(octal <= 255, "invalid mount path escape");
            bytes.push(octal as u8);
            rest = &rest[4..];
        } else {
            bytes.push(rest[0]);
            rest = &rest[1..];
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_vec(bytes).into())
}

fn hosted_github_actions_lane() -> bool {
    std::env::var_os("GITHUB_ACTIONS").is_some_and(|value| value == "true")
        && std::env::var_os("JAIN_RELEASE_CI").is_none_or(|value| value != "1")
}

fn ignore_vanished_proc(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    )
}

pub(super) fn remove(root: &Path, identity: (u64, u64, u32)) -> Result<()> {
    ensure!(
        owned_directory(root)? == identity,
        "retaining changed public source directory"
    );
    let mountinfo = match fs::read_to_string("/proc/self/mountinfo") {
        Ok(text) => Some(text),
        Err(error) if ignore_vanished_proc(&error) && hosted_github_actions_lane() => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(text) = mountinfo {
        for line in text.lines() {
            let mount = mount_path(
                line.split_whitespace()
                    .nth(4)
                    .ok_or_else(|| anyhow::anyhow!("mount record unavailable"))?,
            )?;
            ensure!(!mount.starts_with(root), "retaining mounted public source");
        }
    }
    inspect(root, super::MAX_SOURCE_BYTES, super::MAX_SOURCE_ENTRIES)?;
    // Refuse cleanup while any same-user process still references this clone.
    // Host stays fail-closed. Public GHA may deny sibling /proc entries.
    let proc_entries = match fs::read_dir("/proc") {
        Ok(entries) => Some(entries),
        Err(error) if ignore_vanished_proc(&error) && hosted_github_actions_lane() => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(proc_entries) = proc_entries {
        for entry in proc_entries {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) if ignore_vanished_proc(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            if !path
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().iter().all(u8::is_ascii_digit))
            {
                continue;
            }
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if ignore_vanished_proc(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.uid() != identity.2 {
                continue;
            }
            for name in ["cwd", "root", "exe"] {
                match fs::read_link(path.join(name)) {
                    Ok(target) => ensure!(
                        !target.starts_with(root),
                        "retaining referenced public source"
                    ),
                    Err(error) if ignore_vanished_proc(&error) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            let maps = match fs::read_to_string(path.join("maps")) {
                Ok(maps) => maps,
                Err(error) if ignore_vanished_proc(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            for line in maps.lines() {
                check_mapping(line, root)?;
            }
            let handles = match fs::read_dir(path.join("fd")) {
                Ok(handles) => handles,
                Err(error) if ignore_vanished_proc(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            for handle in handles {
                let handle = match handle {
                    Ok(handle) => handle,
                    Err(error) if ignore_vanished_proc(&error) => continue,
                    Err(error) => return Err(error.into()),
                };
                match fs::read_link(handle.path()) {
                    Ok(target) => {
                        ensure!(!target.starts_with(root), "retaining open public source")
                    }
                    Err(error) if ignore_vanished_proc(&error) => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    ensure!(
        owned_directory(root)? == identity,
        "retaining changed public source directory"
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

pub(super) fn check_mapping(line: &str, root: &Path) -> Result<()> {
    let mut rest = line;
    for _ in 0..5 {
        rest = rest.trim_start();
        rest = rest
            .find(char::is_whitespace)
            .map_or("", |index| &rest[index..]);
    }
    let name = rest.trim_start();
    if name.starts_with('/') {
        // proc maps escapes newlines but leaves literal backslashes alone.
        // Ambiguous pathnames cannot establish safe cleanup custody.
        ensure!(
            !name.contains('\\'),
            "retaining public source: ambiguous mapped pathname"
        );
        ensure!(
            !Path::new(name).starts_with(root),
            "retaining mapped public source"
        );
    }
    Ok(())
}
