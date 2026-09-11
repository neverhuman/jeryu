//! Real local filesystem identity fixtures; no commissioning or operator trust.

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use super::*;
use crate::{DurableRequiredPublisher, RequiredPublisherAction, RequiredPublisherAuthority};

#[derive(Debug)]
struct FixtureAuthority(RequiredPublisherCustody);

impl RequiredPublisherAuthority for FixtureAuthority {
    fn custody(&self) -> &RequiredPublisherCustody {
        &self.0
    }

    fn validate(
        &self,
        _: &RequiredPublisherCustody,
        _: &DurableRequiredPublisher,
        _: RequiredPublisherAction,
        _: DateTime<Utc>,
    ) -> Result<()> {
        Err(ForgeError::WriterUnavailable(
            "no fixture trust gate".into(),
        ))
    }
}

fn opened(directory: &tempfile::TempDir, name: &str) -> ForgeCore {
    ForgeCore::open_managed(directory.path().join(name), directory.path().join("repos")).unwrap()
}

#[test]
fn actual_database_and_lease_descriptors_match_receiving_files() {
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let core = opened(&directory, "forge.sqlite");
    let actual = core.required_publisher_custody().unwrap();
    assert_eq!(actual.process_id, std::process::id());
    assert_eq!(
        actual.database.resource.path,
        directory.path().join("forge.sqlite")
    );
    assert_eq!(actual.storage_root.path, directory.path().join("repos"));
    assert_eq!(actual.writer_leases.len(), 2);
    let mut paths = actual
        .writer_leases
        .iter()
        .map(|file| file.resource.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    let mut expected = vec![
        directory.path().join("forge.sqlite.writer.lock"),
        directory.path().join("repos/.jeryu-writer.lock"),
    ];
    expected.sort();
    assert_eq!(paths, expected);
    for file in std::iter::once(&actual.database).chain(&actual.writer_leases) {
        let metadata = fs::symlink_metadata(&file.resource.path).unwrap();
        assert_eq!(file.resource.device, metadata.dev());
        assert_eq!(file.resource.inode, metadata.ino());
        assert_eq!(file.resource.owner, metadata.uid());
        assert_eq!(file.resource.group, metadata.gid());
        assert_eq!(file.resource.mode, metadata.mode());
        #[cfg(target_os = "linux")]
        {
            let descriptor = fs::metadata(format!("/proc/self/fd/{}", file.descriptor)).unwrap();
            assert_eq!(
                (descriptor.dev(), descriptor.ino()),
                (metadata.dev(), metadata.ino())
            );
            let fdinfo =
                fs::read_to_string(format!("/proc/self/fdinfo/{}", file.descriptor)).unwrap();
            assert!(fdinfo.lines().any(|line| line.starts_with("lock:")
                && line.contains("FLOCK")
                && line.contains("WRITE")));
        }
    }
}

#[test]
fn identical_git_root_cannot_adopt_authority_for_another_database() {
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let core = opened(&directory, "first.sqlite");
    let authority = Arc::new(FixtureAuthority(core.required_publisher_custody().unwrap()));
    drop(core);
    let other = opened(&directory, "second.sqlite");
    assert_eq!(
        other.required_publisher_custody().unwrap().storage_root,
        authority.0.storage_root
    );
    assert!(matches!(
        other.with_required_publisher_authority(authority),
        Err(ForgeError::WriterUnavailable(_))
    ));
}

#[test]
fn attachment_compares_every_incarnation_and_resource_binding() {
    let directory = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let core = opened(&directory, "forge.sqlite");
    let original = core.required_publisher_custody().unwrap();
    for case in 0..11 {
        let mut changed = original.clone();
        match case {
            0 => changed.process_id = changed.process_id.wrapping_add(1),
            1 => changed.database.descriptor += 1,
            2 => changed.database.resource.path = directory.path().join("other.sqlite"),
            3 => changed.database.resource.device += 1,
            4 => changed.database.resource.inode += 1,
            5 => changed.database.resource.owner += 1,
            6 => changed.database.resource.group += 1,
            7 => changed.database.resource.mode ^= 0o040,
            8 => changed.storage_root.inode += 1,
            9 => changed.writer_leases[0].resource.inode += 1,
            10 => {
                changed.writer_leases.pop();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                core.clone()
                    .with_required_publisher_authority(Arc::new(FixtureAuthority(changed))),
                Err(ForgeError::WriterUnavailable(_))
            ),
            "case {case}"
        );
    }
}

#[test]
fn replaced_database_or_lease_path_refuses_surviving_descriptors() {
    for resource in 0..3 {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let core = opened(&directory, "forge.sqlite");
        let before = core.required_publisher_custody().unwrap();
        let selected = if resource == 0 {
            &before.database
        } else {
            &before.writer_leases[resource - 1]
        };
        let preserved = directory.path().join(format!("preserved-{resource}"));
        fs::rename(&selected.resource.path, &preserved).unwrap();
        fs::write(&selected.resource.path, b"unexpected replacement").unwrap();
        assert!(matches!(
            core.required_publisher_custody(),
            Err(ForgeError::WriterUnavailable(_))
        ));
        #[cfg(target_os = "linux")]
        assert_eq!(
            fs::metadata(format!("/proc/self/fd/{}", selected.descriptor))
                .unwrap()
                .ino(),
            selected.resource.inode
        );
        assert!(preserved.exists());
    }
}
