//! Cooperative backing-resource leases. Managed filesystem isolation is separate.

use std::path::{Path, PathBuf};

use crate::{ForgeError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct BackingIdentity {
    pub(super) database: PathBuf,
    pub(super) storage_root: Option<PathBuf>,
}

#[cfg(unix)]
mod unix {
    use std::fs::{self, DirBuilder, File, Metadata, OpenOptions};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    use std::path::Component;

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct FileIdentity {
        device: u64,
        inode: u64,
    }

    impl From<&Metadata> for FileIdentity {
        fn from(metadata: &Metadata) -> Self {
            Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        }
    }

    #[derive(Debug)]
    struct BoundFile {
        path: PathBuf,
        file: File,
        identity: FileIdentity,
        parent_identity: FileIdentity,
    }

    impl BoundFile {
        fn open(path: &Path) -> Result<Self> {
            let parent = path
                .parent()
                .ok_or_else(|| unavailable("missing resource parent"))?;
            let parent_identity = FileIdentity::from(&validate_directory_chain(parent)?);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
                .open(path)
                .map_err(io_error)?;
            let metadata = file.metadata().map_err(io_error)?;
            validate_file(&metadata)?;
            let bound = Self {
                path: path.to_path_buf(),
                file,
                identity: FileIdentity::from(&metadata),
                parent_identity,
            };
            bound.validate()?;
            Ok(bound)
        }

        fn validate(&self) -> Result<()> {
            let parent = self
                .path
                .parent()
                .ok_or_else(|| unavailable("missing resource parent"))?;
            if FileIdentity::from(&validate_directory_chain(parent)?) != self.parent_identity {
                return Err(unavailable("resource parent identity changed"));
            }
            let path = fs::symlink_metadata(&self.path).map_err(io_error)?;
            let descriptor = self.file.metadata().map_err(io_error)?;
            validate_file(&path)?;
            validate_file(&descriptor)?;
            if FileIdentity::from(&path) != self.identity
                || FileIdentity::from(&descriptor) != self.identity
            {
                return Err(unavailable("resource file identity changed"));
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    pub(in super::super) struct WriterLease {
        resources: Vec<BoundFile>,
        database: BoundFile,
        storage_root: Option<(PathBuf, FileIdentity)>,
        process_id: u32,
    }

    impl BackingIdentity {
        pub(in super::super) fn admit(
            database: &Path,
            storage_root: Option<&Path>,
        ) -> Result<Self> {
            let database = absolute_path(database)?;
            if database
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".writer.lock") || name == ".jeryu-writer.lock")
            {
                return Err(unavailable("database path is reserved for writer leases"));
            }
            let parent = database
                .parent()
                .ok_or_else(|| unavailable("missing database parent"))?;
            prepare_directory(parent)?;
            let storage_root = storage_root.map(absolute_path).transpose()?;
            if let Some(root) = &storage_root {
                prepare_directory(root)?;
            }
            Ok(Self {
                database,
                storage_root,
            })
        }
    }

    impl WriterLease {
        pub(in super::super) fn acquire(identity: &BackingIdentity) -> Result<Self> {
            let mut database_lock = identity.database.as_os_str().to_os_string();
            database_lock.push(".writer.lock");
            let mut paths = vec![PathBuf::from(database_lock)];
            if let Some(root) = &identity.storage_root {
                paths.push(root.join(".jeryu-writer.lock"));
            }
            paths.sort();
            if paths.windows(2).any(|pair| pair[0] == pair[1]) || paths.contains(&identity.database)
            {
                return Err(unavailable("database and lease paths overlap"));
            }
            let mut resources = Vec::new();
            for path in paths {
                let bound = BoundFile::open(&path)?;
                bound
                    .file
                    .try_lock()
                    .map_err(|error| unavailable(&format!("resource lease refused: {error}")))?;
                bound.validate()?;
                resources.push(bound);
            }
            // No SQLite open, migration or backfill has occurred before both locks.
            let database = BoundFile::open(&identity.database)?;
            // The sidecar protects the pathname while this lock protects the
            // database inode if a maintenance process renames it. SQLite's Linux
            // POSIX byte-range locks remain independent of this flock lease.
            database
                .file
                .try_lock()
                .map_err(|error| unavailable(&format!("database inode lease refused: {error}")))?;
            database.validate()?;
            let storage_root = identity
                .storage_root
                .as_ref()
                .map(|root| {
                    validate_directory_chain(root)
                        .map(|metadata| (root.clone(), FileIdentity::from(&metadata)))
                })
                .transpose()?;
            let lease = Self {
                resources,
                database,
                storage_root,
                process_id: std::process::id(),
            };
            lease.validate()?;
            Ok(lease)
        }

        pub(in super::super) fn validate(&self) -> Result<()> {
            if self.process_id != std::process::id() {
                return Err(unavailable(
                    "a writer lease cannot be inherited by another process",
                ));
            }
            for resource in &self.resources {
                resource.validate()?;
            }
            self.database.validate()?;
            if let Some((root, identity)) = &self.storage_root
                && FileIdentity::from(&validate_directory_chain(root)?) != *identity
            {
                return Err(unavailable("Git storage root identity changed"));
            }
            Ok(())
        }
    }

    fn absolute_path(path: &Path) -> Result<PathBuf> {
        if path.as_os_str().is_empty() {
            return Err(unavailable("empty backing-resource path"));
        }
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_err(io_error)?.join(path)
        };
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::RootDir | Component::Normal(_) => normalized.push(component.as_os_str()),
                Component::CurDir => {}
                _ => {
                    return Err(unavailable(
                        "backing-resource paths cannot contain parent traversal",
                    ));
                }
            }
        }
        Ok(normalized)
    }

    fn prepare_directory(path: &Path) -> Result<()> {
        let mut current = PathBuf::new();
        let count = path.components().count();
        for (index, component) in path.components().enumerate() {
            current.push(component.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match DirBuilder::new().mode(0o700).create(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(io_error(error)),
                    }
                }
                Err(error) => return Err(io_error(error)),
            }
            directory_metadata(&current, index + 1 < count)?;
        }
        Ok(())
    }

    fn validate_directory_chain(path: &Path) -> Result<Metadata> {
        let mut current = PathBuf::new();
        let count = path.components().count();
        let mut last = None;
        for (index, component) in path.components().enumerate() {
            current.push(component.as_os_str());
            last = Some(directory_metadata(&current, index + 1 < count)?);
        }
        last.ok_or_else(|| unavailable("missing resource directory"))
    }

    fn directory_metadata(path: &Path, ancestor: bool) -> Result<Metadata> {
        let metadata = fs::symlink_metadata(path).map_err(io_error)?;
        if !metadata.file_type().is_dir()
            || (metadata.mode() & 0o022 != 0 && !(ancestor && metadata.mode() & 0o1000 != 0))
        {
            return Err(unavailable(
                "resource directories must be physical and protected from group/other writes",
            ));
        }
        Ok(metadata)
    }

    fn validate_file(metadata: &Metadata) -> Result<()> {
        if !metadata.file_type().is_file() || metadata.nlink() != 1 || metadata.mode() & 0o022 != 0
        {
            return Err(unavailable(
                "resource files must be single-link regular files without group/other writes",
            ));
        }
        Ok(())
    }

    fn io_error(error: std::io::Error) -> ForgeError {
        unavailable(&error.to_string())
    }

    fn unavailable(reason: &str) -> ForgeError {
        ForgeError::WriterUnavailable(reason.to_string())
    }
}

#[cfg(unix)]
pub(super) use unix::WriterLease;

#[cfg(not(unix))]
mod unsupported {
    use super::*;

    #[derive(Debug)]
    pub(in super::super) struct WriterLease;

    impl BackingIdentity {
        pub(in super::super) fn admit(_: &Path, _: Option<&Path>) -> Result<Self> {
            Err(ForgeError::WriterUnavailable(
                "persistent writer custody requires Unix".to_string(),
            ))
        }
    }

    impl WriterLease {
        pub(in super::super) fn acquire(_: &BackingIdentity) -> Result<Self> {
            Err(ForgeError::WriterUnavailable(
                "persistent writer custody requires Unix".to_string(),
            ))
        }

        pub(in super::super) fn validate(&self) -> Result<()> {
            Err(ForgeError::WriterUnavailable(
                "persistent writer custody requires Unix".to_string(),
            ))
        }
    }
}

#[cfg(not(unix))]
pub(super) use unsupported::WriterLease;
