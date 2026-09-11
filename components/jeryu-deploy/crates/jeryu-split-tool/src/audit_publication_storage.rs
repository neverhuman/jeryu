//! Atomic creation of private candidate bundles. No cleanup or publication.
use super::*;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
};

#[derive(Serialize)]
pub(crate) struct Stored {
    pub path: PathBuf,
    pub already_present: bool,
    pub retained_staging: Option<PathBuf>,
}

fn directory(path: &Path) -> Result<(u64, u64, u32)> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && path.canonicalize()? == path
            && metadata.is_dir()
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.mode() & 0o077 == 0,
        "bundle directory must be physical and owner-only"
    );
    Ok((metadata.dev(), metadata.ino(), metadata.uid()))
}

pub(super) fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.nlink() == 1 && before.len() <= limit as u64,
        "bounded ordinary artifact file required"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    let identity = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
            m.nlink(),
        )
    };
    ensure!(
        named.is_file()
            && identity(&before) == identity(&after)
            && identity(&before) == identity(&named)
            && bytes.len() as u64 == before.len(),
        "artifact changed while reading"
    );
    Ok(bytes)
}

fn verify_existing(path: &Path, bundle: &PreparedBundle) -> Result<()> {
    let held = directory(path)?;
    let mut names = std::collections::BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("unexpected bundle filename"))?;
        ensure!(names.insert(name.clone()), "duplicate bundle entry");
        let expected = bundle
            .files
            .get(&name)
            .context("unexpected existing bundle file")?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.is_file() && metadata.uid() == held.2 && metadata.mode() & 0o077 == 0,
            "existing bundle has unsafe file custody"
        );
        ensure!(
            read_file(&entry.path(), MAX_ARTIFACT)? == *expected,
            "immutable bundle conflicts with different bytes"
        );
    }
    ensure!(
        names.len() == bundle.files.len() && directory(path)? == held,
        "existing immutable bundle is incomplete or changed"
    );
    Ok(())
}

fn rename_new(source: &Path, destination: &Path) -> std::io::Result<()> {
    // Keep atomic no-replace behavior and the workspace's unsafe-code prohibition.
    // An unsupported filesystem fails; an overwriting rename is never substituted.
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(Into::into)
}

pub(crate) fn create_once(root: &Path, bundle: &PreparedBundle) -> Result<Stored> {
    let root_identity = directory(root)?;
    ensure!(
        !root.starts_with(&bundle.source_root),
        "private bundle storage must be outside source"
    );
    let destination = root.join(bundle.destination());
    ensure!(
        !destination.starts_with(&bundle.source_root),
        "immutable destination must be outside source"
    );
    let parent = destination
        .parent()
        .context("immutable destination parent")?;
    let relative = parent.strip_prefix(root)?;
    let mut current = root.to_owned();
    for part in relative.components() {
        ensure!(
            matches!(part, std::path::Component::Normal(_)),
            "invalid immutable destination segment"
        );
        current.push(part);
        match fs::DirBuilder::new().mode(0o700).create(&current) {
            Ok(()) => fs::File::open(current.parent().unwrap())?.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        directory(&current)?;
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            verify_existing(&destination, bundle)?;
            ensure!(
                directory(root)? == root_identity,
                "bundle storage root changed"
            );
            return Ok(Stored {
                path: destination,
                already_present: true,
                retained_staging: None,
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let stage = tempfile::Builder::new()
        .prefix(".publication-prepare-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)?
        .keep();
    eprintln!(
        "private bundle staging retained until atomic commit: {}",
        stage.display()
    );
    for (name, bytes) in &bundle.files {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(stage.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    verify_existing(&stage, bundle)?;
    fs::File::open(&stage)?.sync_all()?;
    ensure!(
        directory(root)? == root_identity,
        "bundle storage root changed"
    );
    directory(parent)?;
    let retained_staging = match rename_new(&stage, &destination) {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_existing(&destination, bundle)?;
            Some(stage)
        }
        Err(error) => return Err(error.into()),
    };
    fs::File::open(parent)?.sync_all()?;
    verify_existing(&destination, bundle)?;
    ensure!(
        directory(root)? == root_identity,
        "bundle storage root changed"
    );
    Ok(Stored {
        path: destination,
        already_present: retained_staging.is_some(),
        retained_staging,
    })
}
