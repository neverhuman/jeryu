use std::fs::{self, DirBuilder, File, Metadata};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TestRoot {
    root: PathBuf,
    store: PathBuf,
    held: File,
    identity: Metadata,
    cleaned: bool,
}

pub fn temp_root(name: &str) -> TestRoot {
    let parent = fs::canonicalize(std::env::temp_dir()).expect("physical temporary parent");
    TestRoot::new_in(&parent, name)
}

impl TestRoot {
    fn new_in(parent: &Path, name: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = parent.join(format!(
            "jeryu_cache-{name}-{}-{nanos}-{serial}",
            std::process::id()
        ));
        // Never remove or adopt an existing pathname, even on a name collision.
        DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .expect("exclusive private fixture root");
        let held = File::open(&root).expect("retain fixture directory");
        let identity = held.metadata().expect("fixture identity");
        Self {
            // The adversarial harness may recreate its store. Custody belongs
            // to the private outer directory, whose inode remains unchanged.
            store: root.join("store"),
            root,
            held,
            identity,
            cleaned: false,
        }
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

impl AsRef<Path> for TestRoot {
    fn as_ref(&self) -> &Path {
        &self.store
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            let message = format!(
                "cache fixture cleanup failed; retained {}: {error}",
                self.root.display()
            );
            if std::thread::panicking() {
                // The test already fails. Report cleanup failure without a
                // second panic aborting unrelated concurrently running tests.
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
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    for line in fs::read("/proc/self/mountinfo")?.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let encoded = line
            .split(|byte| byte.is_ascii_whitespace())
            .nth(4)
            .ok_or_else(|| io::Error::other("invalid mountinfo record"))?;
        let mut decoded = Vec::new();
        let mut index = 0;
        while index < encoded.len() {
            if encoded[index] == b'\\' {
                let octal = encoded
                    .get(index + 1..index + 4)
                    .filter(|digits| digits.iter().all(|digit| (b'0'..=b'7').contains(digit)))
                    .ok_or_else(|| io::Error::other("invalid mountinfo escape"))?;
                let value = u16::from(octal[0] - b'0') * 64
                    + u16::from(octal[1] - b'0') * 8
                    + u16::from(octal[2] - b'0');
                decoded.push(
                    u8::try_from(value)
                        .map_err(|_| io::Error::other("invalid mountinfo escape value"))?,
                );
                index += 4;
            } else {
                decoded.push(encoded[index]);
                index += 1;
            }
        }
        if Path::new(OsStr::from_bytes(&decoded)).starts_with(root) {
            return Err(io::Error::other("fixture root contains a mount"));
        }
    }
    Ok(())
}

#[test]
fn cleanup_refuses_replacement_and_links_without_following_targets() {
    use std::os::unix::fs::symlink;

    let mut outer = temp_root("cleanup-test");
    let mut inner = TestRoot::new_in(&outer.root, "owned");
    let sentinel = outer.root.join("sentinel");
    fs::write(&sentinel, b"fixture outside the cleanup root").unwrap();
    fs::create_dir(inner.as_ref()).unwrap();
    let link = inner.as_ref().join("outside-link");
    symlink(&sentinel, &link).unwrap();
    assert!(inner.cleanup().is_err());
    assert!(inner.root.is_dir());
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"fixture outside the cleanup root"
    );
    assert_eq!(fs::read_link(&link).unwrap(), sentinel);
    fs::remove_file(&link).unwrap();

    let moved = outer.root.join("moved");
    fs::rename(&inner.root, &moved).unwrap();
    fs::create_dir(&inner.root).unwrap();
    assert!(inner.cleanup().is_err());
    assert!(moved.is_dir() && inner.root.is_dir());
    fs::remove_dir(&inner.root).unwrap(); // Only our empty replacement.
    fs::rename(&moved, &inner.root).unwrap();
    inner.cleanup().unwrap();
    assert!(fs::symlink_metadata(&inner.root).is_err());
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"fixture outside the cleanup root"
    );
    outer.cleanup().unwrap();
    assert!(fs::symlink_metadata(&outer.root).is_err());
}
