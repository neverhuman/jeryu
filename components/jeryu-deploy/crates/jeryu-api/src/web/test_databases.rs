use std::fs::{self, File, Metadata};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Owns the two auxiliary databases shared by cloned test WebState values.
pub(super) struct TestDatabases {
    root: PathBuf,
    database: PathBuf,
    held: File,
    identity: Metadata,
    cleaned: bool,
}

impl TestDatabases {
    pub(super) fn scratch() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).expect("physical scratch parent");
        Self::new_in(&parent)
    }

    fn new_in(parent: &Path) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("jeryu-web-test-db-")
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

    pub(super) fn work_path(&self) -> &Path {
        &self.database
    }

    pub(super) fn codegraph_path(&self) -> PathBuf {
        self.root.join("codegraph.sqlite")
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
                Some(
                    "work.sqlite"
                        | "work.sqlite-journal"
                        | "work.sqlite-wal"
                        | "work.sqlite-shm"
                        | "codegraph.sqlite"
                        | "codegraph.sqlite-journal"
                        | "codegraph.sqlite-wal"
                        | "codegraph.sqlite-shm"
                )
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

impl Drop for TestDatabases {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            let message = format!(
                "Web database fixture cleanup failed; retained {}: {error}",
                self.root.display()
            );
            if std::thread::panicking() {
                eprintln!("{message}");
            } else {
                panic!("{message}");
            }
        }
    }
}

#[test]
fn database_and_sidecars_are_removed_on_return_and_unwind() {
    for unwind in [false, true] {
        let fixture = TestDatabases::scratch();
        let root = fixture.root.clone();
        for name in [
            "work.sqlite",
            "work.sqlite-journal",
            "work.sqlite-wal",
            "work.sqlite-shm",
            "codegraph.sqlite",
            "codegraph.sqlite-journal",
            "codegraph.sqlite-wal",
            "codegraph.sqlite-shm",
        ] {
            fs::write(root.join(name), b"synthetic cleanup fixture").unwrap();
        }
        let result = std::panic::catch_unwind(move || {
            let _fixture = fixture;
            assert!(!unwind, "intentional fixture unwind");
        });
        assert_eq!(result.is_err(), unwind);
        assert_eq!(
            fs::symlink_metadata(root).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}

#[test]
fn cloned_web_states_keep_both_databases_until_the_last_owner_drops() {
    use super::WebState;
    use jeryu_core::ForgeCore;

    let first = WebState::new(ForgeCore::new());
    let second = first.clone();
    let independent = WebState::new(ForgeCore::new());
    let root = first._test_databases.root.clone();
    assert_ne!(root, independent._test_databases.root);
    let work = first._test_databases.work_path().to_path_buf();
    let codegraph = first._test_databases.codegraph_path();
    assert!(work.is_file() && codegraph.is_file());
    drop(first);
    assert!(work.is_file() && codegraph.is_file());
    assert_eq!(second.work.list(Default::default()).unwrap().len(), 0);
    assert!(!second.codegraph_store.schema_version().unwrap().is_empty());
    drop(second);
    assert_eq!(
        fs::symlink_metadata(root).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn cleanup_refuses_links_unknown_children_and_root_replacement() {
    use std::os::unix::fs::{DirBuilderExt, symlink};

    let outer = TestDatabases::scratch();
    let mut inner = TestDatabases::new_in(&outer.root);
    fs::write(outer.work_path(), b"outside the inner cleanup root").unwrap();
    symlink(outer.work_path(), inner.work_path()).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::read_link(inner.work_path()).unwrap(), outer.work_path());
    fs::remove_file(inner.work_path()).unwrap();

    fs::hard_link(outer.work_path(), inner.work_path()).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::symlink_metadata(inner.work_path()).unwrap().nlink(), 2);
    fs::remove_file(inner.work_path()).unwrap();

    fs::write(inner.work_path(), b"owned database fixture").unwrap();
    let unexpected = inner.root.join("unexpected");
    fs::create_dir(&unexpected).unwrap();
    assert!(inner.cleanup().is_err());
    assert!(inner.work_path().is_file());
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
        fs::read(outer.work_path()).unwrap(),
        b"outside the inner cleanup root"
    );
}
