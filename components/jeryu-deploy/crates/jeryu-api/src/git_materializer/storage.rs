//! Held, private creation receipts. Failed writes stay in custody for diagnosis.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, ensure};
use rustix::fs::{AtFlags, FlockOperation, Mode, OFlags, RenameFlags};
use serde::{Serialize, de::DeserializeOwned};

pub(crate) struct Directory {
    pub(crate) file: File,
    pub(crate) path: PathBuf,
}

impl Directory {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        ensure!(
            !path.components().any(|part| part == Component::ParentDir),
            "creation path must not contain parent traversal"
        );
        let absolute = std::path::absolute(path)?;
        let uid = fs::metadata("/proc/self")?.uid();
        let mut directory = Self {
            file: File::from(rustix::fs::open(
                "/",
                OFlags::RDONLY | OFlags::DIRECTORY,
                Mode::empty(),
            )?),
            path: PathBuf::from("/"),
        };
        for component in absolute.components() {
            match component {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(name) => {
                    match rustix::fs::mkdirat(&directory.file, name, Mode::RWXU) {
                        Ok(()) => directory.file.sync_all()?,
                        Err(rustix::io::Errno::EXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    let file = File::from(rustix::fs::openat(
                        &directory.file,
                        name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                        Mode::empty(),
                    )?);
                    let metadata = file.metadata()?;
                    ensure!(
                        metadata.uid() == 0 || metadata.uid() == uid,
                        "foreign creation directory"
                    );
                    ensure!(
                        metadata.mode() & 0o022 == 0
                            || (metadata.uid() == 0 && metadata.mode() & 0o1000 != 0),
                        "writable creation ancestor"
                    );
                    directory = Self {
                        file,
                        path: directory.path.join(name),
                    };
                }
                _ => anyhow::bail!("creation path must not contain parent traversal"),
            }
        }
        let metadata = directory.file.metadata()?;
        ensure!(
            metadata.uid() == uid && metadata.mode() & 0o022 == 0,
            "creation directory must be owned and protected"
        );
        directory.check()?;
        Ok(directory)
    }

    pub(crate) fn child(&self, name: &str) -> Result<Self> {
        ensure!(
            matches!(
                Path::new(name).components().next(),
                Some(Component::Normal(_))
            ) && Path::new(name).components().count() == 1,
            "invalid creation directory name"
        );
        self.check()?;
        Self::open(&self.path.join(name))
    }

    pub(crate) fn check(&self) -> Result<()> {
        let held = self.file.metadata()?;
        let named = fs::symlink_metadata(&self.path)?;
        ensure!(
            named.is_dir() && (held.dev(), held.ino()) == (named.dev(), named.ino()),
            "creation directory replaced"
        );
        ensure!(
            held.uid() == fs::metadata("/proc/self")?.uid() && held.mode() & 0o022 == 0,
            "creation directory became unsafe"
        );
        Ok(())
    }

    pub(crate) fn exists(&self, name: &str) -> Result<bool> {
        self.check()?;
        match rustix::fs::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(rustix::io::Errno::NOENT) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn lock(&self, name: &str) -> Result<File> {
        self.check()?;
        let file = File::from(rustix::fs::openat(
            &self.file,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::RUSR | Mode::WUSR,
        )?);
        check_file(&file)?;
        rustix::fs::flock(&file, FlockOperation::NonBlockingLockExclusive)?;
        let named = rustix::fs::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW)?;
        let held = file.metadata()?;
        ensure!(
            held.dev() == named.st_dev && held.ino() == named.st_ino,
            "creation lock replaced"
        );
        file.sync_all()?;
        self.file.sync_all()?;
        Ok(file)
    }

    pub(crate) fn read<T: DeserializeOwned>(&self, name: &str) -> Result<T> {
        self.check()?;
        let mut file = File::from(rustix::fs::openat(
            &self.file,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )?);
        check_file(&file)?;
        let before = file.metadata()?;
        ensure!(before.len() <= 16 * 1024, "oversized creation receipt");
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(16 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        let after = file.metadata()?;
        let named = rustix::fs::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW)?;
        ensure!(
            before.len() == bytes.len() as u64
                && before.len() == after.len()
                && before.mtime_nsec() == after.mtime_nsec()
                && before.mtime() == after.mtime()
                && before.ctime_nsec() == after.ctime_nsec()
                && before.ctime() == after.ctime()
                && before.ino() == named.st_ino
                && before.dev() == named.st_dev,
            "creation receipt changed while reading"
        );
        self.check()?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub(crate) fn write<T: Serialize>(&self, name: &str, value: &T, replace: bool) -> Result<()> {
        self.check()?;
        let scratch = format!("pending-{}", uuid::Uuid::new_v4());
        let mut file = File::from(rustix::fs::openat(
            &self.file,
            &scratch,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW,
            Mode::RUSR | Mode::WUSR,
        )?);
        check_file(&file)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        self.check()?;
        rustix::fs::renameat_with(
            &self.file,
            &scratch,
            &self.file,
            name,
            if replace {
                RenameFlags::empty()
            } else {
                RenameFlags::NOREPLACE
            },
        )?;
        self.file.sync_all()?;
        Ok(())
    }
}

fn check_file(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.mode() & 0o7777 == 0o600
            && metadata.uid() == fs::metadata("/proc/self")?.uid(),
        "unsafe creation receipt or lock"
    );
    Ok(())
}

/// Sync only physical files and directories; a staging tree cannot redirect Git
/// or durability writes through links or special files.
pub(crate) fn sync_tree(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.uid() == fs::metadata("/proc/self")?.uid() && metadata.mode() & 0o022 == 0,
        "unsafe creation tree ownership at {} (uid {}, mode {:o})",
        path.display(),
        metadata.uid(),
        metadata.mode() & 0o7777
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    } else {
        ensure!(
            metadata.is_file() && metadata.nlink() == 1,
            "creation tree contains a link or special file"
        );
    }
    let file = File::from(rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )?);
    ensure!(
        (file.metadata()?.dev(), file.metadata()?.ino()) == (metadata.dev(), metadata.ino()),
        "creation tree replaced"
    );
    file.sync_all()?;
    Ok(())
}
