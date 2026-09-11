//! Private runtime-data custody for the stopped-server restore proof.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Snapshot = BTreeMap<PathBuf, (u32, Vec<u8>)>;

pub struct RecoveryFixture {
    root: PathBuf,
    held: File,
    identity: Metadata,
    cleaned: bool,
}

fn identity(metadata: &Metadata) -> (u64, u64, u32, u32, u32) {
    (
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.gid(),
        metadata.mode(),
    )
}

impl RecoveryFixture {
    pub fn temporary() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).expect("physical temporary parent");
        let directory = tempfile::Builder::new()
            .prefix("jeryu-recovery-test-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(parent)
            .expect("private runtime fixture");
        let held = File::open(directory.path()).expect("retain runtime fixture identity");
        let identity = held.metadata().expect("runtime fixture identity");
        // TempDir relinquishes cleanup before any application state is created.
        Self {
            root: directory.keep(),
            held,
            identity,
            cleaned: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    fn check_root(&self) -> io::Result<()> {
        let current = fs::symlink_metadata(&self.root)?;
        if !current.is_dir()
            || current.file_type().is_symlink()
            || fs::canonicalize(&self.root)? != self.root
            || identity(&current) != identity(&self.identity)
            || identity(&self.held.metadata()?) != identity(&self.identity)
            || current.mode() & 0o7777 != 0o700
        {
            return Err(io::Error::other("runtime fixture root custody changed"));
        }
        Ok(())
    }

    fn check_mounts(&self) -> io::Result<()> {
        // A bind mount can retain the same device; device checks alone are insufficient.
        for line in fs::read_to_string("/proc/self/mountinfo")?.lines() {
            let encoded = line
                .split_whitespace()
                .nth(4)
                .ok_or_else(|| io::Error::other("invalid mount identity"))?;
            let decoded = encoded
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\");
            if Path::new(&decoded).starts_with(&self.root) {
                return Err(io::Error::other("runtime fixture contains a mount"));
            }
        }
        Ok(())
    }

    fn entries(&self, path: &Path, entries: &mut Vec<(PathBuf, Metadata)>) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink()
            || (!metadata.is_dir() && !metadata.is_file())
            || metadata.dev() != self.identity.dev()
            || metadata.uid() != self.identity.uid()
            || (metadata.is_file() && metadata.nlink() != 1)
        {
            return Err(io::Error::other(
                "runtime fixture contains a link or unowned entry",
            ));
        }
        let directory = metadata.is_dir();
        entries.push((path.to_owned(), metadata));
        if directory {
            for child in fs::read_dir(path)? {
                self.entries(&child?.path(), entries)?;
            }
        }
        Ok(())
    }

    pub fn snapshot(&self, data: &Path) -> io::Result<Snapshot> {
        self.check_root()?;
        self.check_mounts()?;
        if data.parent() != Some(self.path()) {
            return Err(io::Error::other("snapshot must own a direct runtime child"));
        }
        let mut entries = Vec::new();
        self.entries(data, &mut entries)?;
        entries
            .into_iter()
            .map(|(path, metadata)| {
                let digest = if metadata.is_file() {
                    Sha256::digest(fs::read(&path)?).to_vec()
                } else {
                    Vec::new()
                };
                Ok((
                    path.strip_prefix(data).expect("runtime child").to_owned(),
                    (metadata.mode(), digest),
                ))
            })
            .collect()
    }

    fn check_archive(&self, path: &Path, held: &File, original: &Metadata) -> io::Result<()> {
        for current in [fs::symlink_metadata(path)?, held.metadata()?] {
            if !current.is_file()
                || current.file_type().is_symlink()
                || current.nlink() != 1
                || current.mode() & 0o7777 != 0o600
                || current.uid() != self.identity.uid()
                || identity(&current) != identity(original)
            {
                return Err(io::Error::other("runtime archive custody changed"));
            }
        }
        Ok(())
    }

    /// Call only after the owning server and every writer have stopped and been reaped.
    pub fn restore_stopped_data(&self, data: &Path, restored: &Path) -> io::Result<()> {
        let expected = self.snapshot(data)?;
        if restored.parent() != Some(self.path()) {
            return Err(io::Error::other(
                "restore must own a new direct runtime child",
            ));
        }
        let archive = self.root.join("stopped-data.tar");
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&archive)?;
        let archive_identity = file.metadata()?;
        self.check_archive(&archive, &file, &archive_identity)?;
        let created = Command::new("tar")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(["--create", "--file", "-", "--directory"])
            .arg(data)
            .arg(".")
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(Stdio::piped())
            .output()?;
        if !created.status.success() {
            return Err(io::Error::other(
                "stopped data archive creation failed; fixture retained",
            ));
        }
        file.sync_all()?;
        file.seek(SeekFrom::Start(0))?;
        let mut archive_bytes = Vec::new();
        file.read_to_end(&mut archive_bytes)?;
        let archive_digest = Sha256::digest(&archive_bytes);
        file.seek(SeekFrom::Start(0))?;
        if self.snapshot(data)? != expected {
            return Err(io::Error::other(
                "source data changed during stopped backup",
            ));
        }
        fs::DirBuilder::new().mode(0o700).create(restored)?;
        // Creation, hashing and extraction use clones of the original create_new
        // descriptor. Admit its path and held identity again immediately before use.
        self.check_archive(&archive, &file, &archive_identity)?;
        let extracted = Command::new("tar")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args([
                "--extract",
                "--file",
                "-",
                "--same-permissions",
                "--no-same-owner",
                "--directory",
            ])
            .arg(restored)
            .stdin(file.try_clone()?)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;
        file.seek(SeekFrom::Start(0))?;
        archive_bytes.clear();
        file.read_to_end(&mut archive_bytes)?;
        self.check_archive(&archive, &file, &archive_identity)?;
        let source_unchanged = self.snapshot(data)? == expected;
        if !extracted.status.success()
            || Sha256::digest(&archive_bytes) != archive_digest
            || !source_unchanged
            || self.snapshot(restored)? != expected
        {
            return Err(io::Error::other(
                "restored runtime bytes or modes differ; fixture retained",
            ));
        }
        Ok(())
    }

    fn check_process_references(&self) -> io::Result<()> {
        // One same-user /proc pass; never signal processes or wait for them here.
        // This excludes ordinary live consumers, not hostile same-user races.
        let this_process = std::process::id().to_string();
        let held_descriptor = self.held.as_raw_fd().to_string();
        for entry in fs::read_dir("/proc")? {
            let path = entry?.path();
            if !path
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().iter().all(u8::is_ascii_digit))
            {
                continue;
            }
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        || error.kind() == io::ErrorKind::PermissionDenied =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            if metadata.uid() != self.identity.uid() {
                continue;
            }
            for name in ["cwd", "root", "exe"] {
                match fs::read_link(path.join(name)) {
                    Ok(target) if target.starts_with(&self.root) => {
                        return Err(io::Error::other("live process references runtime fixture"));
                    }
                    Ok(_) => {}
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound
                            || error.kind() == io::ErrorKind::PermissionDenied => {}
                    Err(error) => return Err(error),
                }
            }
            match fs::read_to_string(path.join("maps")) {
                Ok(maps) => {
                    for line in maps.lines() {
                        check_mapping(line, &self.root)?;
                    }
                }
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        || error.kind() == io::ErrorKind::PermissionDenied => {}
                Err(error) => return Err(error),
            }
            let handles = match fs::read_dir(path.join("fd")) {
                Ok(handles) => handles,
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound
                        || error.kind() == io::ErrorKind::PermissionDenied =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            for handle in handles {
                let handle = match handle {
                    Ok(handle) => handle,
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound
                            || error.kind() == io::ErrorKind::PermissionDenied =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                match fs::read_link(handle.path()) {
                    Ok(target) => {
                        let own_held_root = path.file_name() == Some(this_process.as_ref())
                            && handle.file_name() == held_descriptor.as_str();
                        if own_held_root {
                            let held = fs::metadata(handle.path())?;
                            if target != self.root
                                || !held.is_dir()
                                || identity(&held) != identity(&self.identity)
                            {
                                return Err(io::Error::other(
                                    "held runtime root descriptor changed",
                                ));
                            }
                        } else if target.starts_with(&self.root) {
                            return Err(io::Error::other("open runtime fixture handle"));
                        }
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound
                            || error.kind() == io::ErrorKind::PermissionDenied => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    fn cleanup(&mut self) -> io::Result<()> {
        self.check_root()?;
        self.check_mounts()?;
        let mut entries = Vec::new();
        self.entries(&self.root, &mut entries)?;
        self.check_process_references()?;
        self.check_root()?;
        self.check_mounts()?;
        // Preflight the whole tree before removing anything, then recheck each known
        // identity. Unlink/rmdir never recursively follow a substituted directory.
        for (path, previous) in entries.into_iter().rev() {
            self.check_root()?;
            let current = fs::symlink_metadata(&path)?;
            if identity(&current) != identity(&previous)
                || (current.is_file() && current.nlink() != 1)
            {
                return Err(io::Error::other("runtime fixture entry custody changed"));
            }
            if current.is_dir() {
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        self.cleaned = true;
        Ok(())
    }

    /// The caller authenticates success and stops/reaps all owned processes first.
    pub fn finish(mut self) {
        self.cleanup()
            .expect("successful runtime fixture cleanup refused; retained remaining state");
    }
}

fn check_mapping(line: &str, root: &Path) -> io::Result<()> {
    let mut rest = line;
    for _ in 0..5 {
        rest = rest.trim_start();
        rest = rest
            .find(char::is_whitespace)
            .map_or("", |index| &rest[index..]);
    }
    let name = rest.trim_start();
    if name.starts_with('/') {
        // /proc maps escapes newlines but leaves literal backslashes unchanged.
        // Keep spaces intact; ambiguous backslashes cannot prove safe custody.
        if name.contains('\\') {
            return Err(io::Error::other(
                "ambiguous mapped pathname; runtime fixture retained",
            ));
        }
        if Path::new(name).starts_with(root) {
            return Err(io::Error::other("mapped runtime fixture file"));
        }
    }
    Ok(())
}

impl Drop for RecoveryFixture {
    fn drop(&mut self) {
        if !self.cleaned {
            eprintln!(
                "Runtime recovery fixture retained: {} (device={}, inode={}, uid={}, gid={}, mode={:o})",
                self.root.display(),
                self.identity.dev(),
                self.identity.ino(),
                self.identity.uid(),
                self.identity.gid(),
                self.identity.mode()
            );
        }
    }
}

#[test]
fn failed_runtime_fixture_is_retained_with_original_identity() {
    let fixture = RecoveryFixture::temporary();
    let root = fixture.path().to_owned();
    let original = identity(&fixture.identity);
    fs::write(root.join("fixture-data"), b"synthetic retained state").unwrap();
    let result = std::panic::catch_unwind(move || {
        let _fixture = fixture;
        panic!("intentional runtime fixture failure");
    });
    assert!(result.is_err());
    let metadata = fs::symlink_metadata(&root).unwrap();
    assert_eq!(identity(&metadata), original);
    assert_eq!(metadata.mode() & 0o7777, 0o700);
    assert!(fs::read(root.join("fixture-data")).unwrap() == b"synthetic retained state");
    // Deliberately retained for reviewed retirement; do not erase failed evidence.
}

#[test]
fn successful_cleanup_refuses_links_and_live_handles_before_removing_owned_bytes() {
    use std::os::unix::fs::symlink;
    let mut fixture = RecoveryFixture::temporary();
    let root = fixture.path().to_owned();
    let data = root.join("fixture-data");
    let link = root.join("unexpected-link");
    fs::write(&data, b"synthetic cleanup state").unwrap();
    symlink(&data, &link).unwrap();
    assert!(fixture.cleanup().is_err());
    assert_eq!(fs::read_link(&link).unwrap(), data);
    assert!(data.is_file());
    fs::remove_file(&link).unwrap(); // Only the link this test just verified.
    fs::hard_link(&data, &link).unwrap();
    assert!(fixture.cleanup().is_err());
    assert_eq!(fs::symlink_metadata(&link).unwrap().nlink(), 2);
    assert_eq!(
        fs::symlink_metadata(&link).unwrap().ino(),
        fs::metadata(&data).unwrap().ino()
    );
    fs::remove_file(&link).unwrap(); // Only our verified extra hard link.
    let open_file = File::open(&data).unwrap();
    assert!(
        fixture
            .cleanup()
            .unwrap_err()
            .to_string()
            .contains("open runtime fixture handle")
    );
    assert!(fs::read(&data).unwrap() == b"synthetic cleanup state");
    drop(open_file);
    let extra_root = fixture.held.try_clone().unwrap();
    assert!(
        fixture
            .cleanup()
            .unwrap_err()
            .to_string()
            .contains("open runtime fixture handle")
    );
    assert!(data.is_file());
    drop(extra_root);
    fixture.finish();
    assert_eq!(
        fs::symlink_metadata(root).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
}

#[test]
fn mapped_paths_preserve_spaces_and_refuse_ambiguous_escapes() {
    let root = Path::new("/tmp/runtime fixture");
    assert!(
        check_mapping(
            "1000-2000 r--p 0 00:01 42 /tmp/runtime fixture/a file",
            root
        )
        .is_err()
    );
    assert!(
        check_mapping(
            "1000-2000 r--p 0 00:01 42 /tmp/unrelated fixture/a file",
            root
        )
        .is_ok()
    );
    assert!(
        check_mapping(
            "1000-2000 r--p 0 00:01 42 /tmp/runtime\\040fixture/file",
            root
        )
        .is_err()
    );
    assert!(check_mapping("1000-2000 rw-p 0 00:00 0 [heap]", root).is_ok());
}
