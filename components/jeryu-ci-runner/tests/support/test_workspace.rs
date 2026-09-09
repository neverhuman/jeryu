use std::fs::{self, DirBuilder, File, Metadata};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Owns a private outer directory while the runner creates its workspace child.
pub struct TestWorkspace {
    root: PathBuf,
    workspace: PathBuf,
    held: File,
    identity: Metadata,
    cleaned: bool,
}

impl TestWorkspace {
    pub fn new(name: &str) -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).expect("physical temporary parent");
        Self::new_in(&parent, name)
    }

    fn new_in(parent: &Path, name: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = parent.join(format!(
            "jeryu-runner-test-{name}-{}-{nanos}-{serial}",
            std::process::id()
        ));
        // Never remove or adopt an existing pathname on a name collision.
        DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .expect("exclusive private fixture root");
        let held = File::open(&root).expect("retain fixture directory");
        let identity = held.metadata().expect("fixture identity");
        Self {
            workspace: root.join("workspace"),
            root,
            held,
            identity,
            cleaned: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.workspace
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
            return Err(io::Error::other("fixture root identity or custody changed"));
        }
        Ok(())
    }

    fn cleanup(&mut self) -> io::Result<()> {
        if self.cleaned {
            return Ok(());
        }
        self.check_root()?;
        #[cfg(target_os = "linux")]
        reject_mounts(&self.root)?;
        inspect_tree(&self.root, &self.identity)?;
        self.check_root()?;
        fs::remove_dir_all(&self.root)?;
        self.cleaned = true;
        Ok(())
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            let message = format!(
                "runner fixture cleanup failed; retained {}: {error}",
                self.root.display()
            );
            if std::thread::panicking() {
                // Preserve the failing assertion; a second panic would abort
                // the test process and prevent unrelated guards from running.
                eprintln!("{message}");
            } else {
                panic!("{message}");
            }
        }
    }
}

fn inspect_tree(path: &Path, root: &Metadata) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || metadata.dev() != root.dev()
        || metadata.uid() != root.uid()
        || (!metadata.is_dir() && !metadata.is_file())
        || (metadata.is_file() && metadata.nlink() != 1)
    {
        return Err(io::Error::other(
            "fixture contains a link, foreign node, mount or special file",
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            inspect_tree(&entry?.path(), root)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn reject_mounts(root: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    // Compare in mountinfo's escaped representation, including bind mounts
    // whose device ID would pass the ordinary tree inspection.
    let mut encoded = Vec::new();
    for byte in root.as_os_str().as_bytes() {
        match byte {
            b' ' => encoded.extend_from_slice(b"\\040"),
            b'\t' => encoded.extend_from_slice(b"\\011"),
            b'\n' => encoded.extend_from_slice(b"\\012"),
            b'\\' => encoded.extend_from_slice(b"\\134"),
            _ => encoded.push(*byte),
        }
    }
    for line in fs::read("/proc/self/mountinfo")?.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let mount = line
            .split(|byte| byte.is_ascii_whitespace())
            .nth(4)
            .ok_or_else(|| io::Error::other("invalid mountinfo record"))?;
        if mount == encoded
            || mount
                .strip_prefix(encoded.as_slice())
                .is_some_and(|suffix| suffix.starts_with(b"/"))
        {
            return Err(io::Error::other("fixture root contains a mount"));
        }
    }
    Ok(())
}

#[test]
fn workspace_is_removed_on_return_and_unwind() {
    for unwind in [false, true] {
        let fixture = TestWorkspace::new("cleanup with space");
        let root = fixture.root.clone();
        assert!(!fixture.path().exists());
        fs::create_dir(fixture.path()).unwrap();
        fs::write(fixture.path().join("output"), b"runner output").unwrap();
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
fn cleanup_refuses_links_and_replacement_without_touching_external_files() {
    use std::os::unix::fs::symlink;

    let outer = TestWorkspace::new("custody-test");
    let mut inner = TestWorkspace::new_in(&outer.root, "owned");
    let sentinel = outer.root.join("sentinel");
    fs::write(&sentinel, b"outside the inner cleanup root").unwrap();
    fs::create_dir(inner.path()).unwrap();
    let link = inner.path().join("outside-link");
    symlink(&sentinel, &link).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::read_link(&link).unwrap(), sentinel);
    fs::remove_file(&link).unwrap();

    fs::hard_link(&sentinel, &link).unwrap();
    assert!(inner.cleanup().is_err());
    assert_eq!(fs::symlink_metadata(&link).unwrap().nlink(), 2);
    fs::remove_file(&link).unwrap();

    let moved = outer.root.join("moved");
    fs::rename(&inner.root, &moved).unwrap();
    DirBuilder::new().mode(0o700).create(&inner.root).unwrap();
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
        fs::read(&sentinel).unwrap(),
        b"outside the inner cleanup root"
    );
}
