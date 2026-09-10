use std::fs::{self, File, Metadata};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Owns one test database and its SQLite sidecars until every store use ends.
pub struct TestDatabase {
    root: PathBuf,
    database: PathBuf,
    held: File,
    identity: Metadata,
    cleaned: bool,
}

impl TestDatabase {
    pub fn temporary() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).expect("physical temporary parent");
        Self::new_in(&parent)
    }

    fn new_in(parent: &Path) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("jeryu-work-test-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(parent)
            .expect("exclusive private database directory");
        let held = File::open(directory.path()).expect("retain fixture directory");
        let identity = held.metadata().expect("fixture identity");
        // This guard takes ownership of cleanup. TempDir must not recursively
        // remove a substituted path after our custody check refuses it.
        let root = directory.keep();
        Self {
            database: root.join("work.sqlite"),
            root,
            held,
            identity,
            cleaned: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.database
    }

    fn check_root(&self) -> io::Result<()> {
        let current = fs::symlink_metadata(&self.root)?;
        let held = self.held.metadata()?;
        let identity = |metadata: &Metadata| {
            (
                metadata.dev(),
                metadata.ino(),
                metadata.uid(),
                metadata.gid(),
                metadata.mode(),
            )
        };
        if !current.is_dir()
            || current.file_type().is_symlink()
            || fs::canonicalize(&self.root)? != self.root
            || identity(&current) != identity(&self.identity)
            || identity(&held) != identity(&self.identity)
            || current.mode() & 0o7777 != 0o700
        {
            return Err(io::Error::other("database fixture root custody changed"));
        }
        Ok(())
    }

    fn cleanup(&mut self) -> io::Result<()> {
        if self.cleaned {
            return Ok(());
        }
        self.check_root()?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if !matches!(
                entry.file_name().to_str(),
                Some("work.sqlite" | "work.sqlite-journal" | "work.sqlite-wal" | "work.sqlite-shm")
            ) || !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.dev() != self.identity.dev()
                || metadata.uid() != self.identity.uid()
                || metadata.nlink() != 1
            {
                return Err(io::Error::other(
                    "database fixture contains an unexpected entry or link",
                ));
            }
            files.push(entry.path());
        }
        self.check_root()?;
        // Flat unlink/rmdir cannot traverse a nested directory or mounted tree.
        // A mounted file or root makes the operation fail and is reported.
        for file in files {
            fs::remove_file(file)?;
        }
        fs::remove_dir(&self.root)?;
        self.cleaned = true;
        Ok(())
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if !self.cleaned {
                eprintln!(
                    "Work database fixture retained after panic: {} (device={}, inode={}, uid={}, gid={}, mode={:o})",
                    self.root.display(),
                    self.identity.dev(),
                    self.identity.ino(),
                    self.identity.uid(),
                    self.identity.gid(),
                    self.identity.mode(),
                );
            }
            return;
        }
        if let Err(error) = self.cleanup() {
            panic!(
                "Work database fixture cleanup failed; retained {}: {error}",
                self.root.display()
            );
        }
    }
}

#[test]
fn database_and_sidecars_are_cleaned_on_success_and_retained_on_unwind() {
    for unwind in [false, true] {
        let fixture = TestDatabase::temporary();
        let root = fixture.root.clone();
        let identity = (
            fixture.identity.dev(),
            fixture.identity.ino(),
            fixture.identity.uid(),
            fixture.identity.gid(),
            fixture.identity.mode(),
        );
        let names = [
            "work.sqlite",
            "work.sqlite-journal",
            "work.sqlite-wal",
            "work.sqlite-shm",
        ];
        for name in names {
            fs::write(root.join(name), b"synthetic cleanup fixture").unwrap();
        }
        let result = std::panic::catch_unwind(move || {
            let _fixture = fixture;
            assert!(!unwind, "intentional fixture unwind");
        });
        assert_eq!(result.is_err(), unwind);
        if unwind {
            let current = fs::symlink_metadata(&root).unwrap();
            assert!(current.is_dir() && !current.file_type().is_symlink());
            assert_eq!(
                (
                    current.dev(),
                    current.ino(),
                    current.uid(),
                    current.gid(),
                    current.mode()
                ),
                identity
            );
            assert_eq!(current.mode() & 0o7777, 0o700);
            for name in names {
                let path = root.join(name);
                let metadata = fs::symlink_metadata(&path).unwrap();
                assert!(metadata.is_file() && !metadata.file_type().is_symlink());
                assert_eq!(metadata.nlink(), 1);
                assert!(
                    fs::read(path).unwrap() == b"synthetic cleanup fixture",
                    "retained fixture bytes changed"
                );
            }
            // The failed fixture stays private for independently reviewed retirement.
        } else {
            assert_eq!(
                fs::symlink_metadata(root).unwrap_err().kind(),
                io::ErrorKind::NotFound
            );
        }
    }
}

#[test]
fn cleanup_refuses_links_unknown_children_and_root_replacement() {
    use std::os::unix::fs::{DirBuilderExt, symlink};

    let outer = TestDatabase::temporary();
    let mut inner = TestDatabase::new_in(&outer.root);
    fs::write(outer.path(), b"outside the inner cleanup root").unwrap();
    symlink(outer.path(), inner.path()).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::read_link(inner.path()).unwrap(), outer.path());
    fs::remove_file(inner.path()).unwrap();

    fs::hard_link(outer.path(), inner.path()).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::symlink_metadata(inner.path()).unwrap().nlink(), 2);
    fs::remove_file(inner.path()).unwrap();

    fs::write(inner.path(), b"owned database fixture").unwrap();
    let unexpected = inner.root.join("unexpected");
    fs::create_dir(&unexpected).unwrap();
    assert!(inner.cleanup().is_err());
    assert!(inner.path().is_file());
    fs::remove_dir(unexpected).unwrap(); // Only our known empty child.

    let moved = outer.root.join("moved");
    fs::rename(&inner.root, &moved).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&inner.root)
        .unwrap();
    assert!(inner.cleanup().is_err());
    assert!(moved.is_dir() && inner.root.is_dir());
    fs::remove_dir(&inner.root).unwrap(); // Only our empty replacement.
    fs::rename(&moved, &inner.root).unwrap();
    inner.cleanup().unwrap();
    assert_eq!(
        fs::symlink_metadata(&inner.root).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        fs::read(outer.path()).unwrap(),
        b"outside the inner cleanup root"
    );
}
