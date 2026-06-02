//! RUNG 2 — round-trip the jailgun tar validators in `jeryu_runnerd::workcell`.
//!
//! A CLEAN subtree of `ArchiveEntry::File`/`Directory` rooted under an approved
//! repo root must round-trip: import validation passes AND the corresponding
//! export paths validate. An ADVERSARIAL set (parent traversal, an absolute
//! path, a Symlink entry, a CharacterDevice entry) must each be REJECTED with
//! the `workcell_tar_path_denied` reason.
//!
//! These exercise the EXISTING public API
//! (`validate_import_archive` / `validate_export_paths` / `ArchiveEntry` /
//! `ArchiveEntryKind`) without changing any validation behavior.
//!
//! Run: `cargo test -p jeryu-runnerd jailgun`

use jeryu_runnerd::workcell::{
    ArchiveEntry, ArchiveEntryKind, validate_export_paths, validate_import_archive,
};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from("/workspace/core/web")
}

/// A clean tree of files and directories that all live under the repo root.
fn clean_tree() -> Vec<ArchiveEntry> {
    vec![
        ArchiveEntry::new("src", ArchiveEntryKind::Directory),
        ArchiveEntry::new("src/lib.rs", ArchiveEntryKind::File),
        ArchiveEntry::new("src/util/mod.rs", ArchiveEntryKind::File),
        ArchiveEntry::new("Cargo.toml", ArchiveEntryKind::File),
        ArchiveEntry::new("tests", ArchiveEntryKind::Directory),
        ArchiveEntry::new("tests/it.rs", ArchiveEntryKind::File),
    ]
}

#[test]
fn jailgun_clean_subtree_round_trips_import_and_export() {
    let allowed_roots = vec![repo_root()];
    let destination = repo_root();

    // Import: a clean File/Directory subtree under the approved root validates.
    validate_import_archive(&clean_tree(), &destination, &allowed_roots)
        .expect("clean File/Directory subtree must import-validate");

    // Export: the same entries, resolved to absolute paths under the repo root,
    // validate as export paths (the round trip).
    let export_paths: Vec<PathBuf> = clean_tree()
        .iter()
        .map(|entry| destination.join(&entry.path))
        .collect();
    validate_export_paths(&export_paths, &allowed_roots)
        .expect("export paths under the approved root must validate");
}

#[test]
fn jailgun_adversarial_entries_are_rejected_with_tar_path_denied() {
    let allowed_roots = vec![repo_root()];
    let destination = repo_root();

    // The required adversarial set: parent traversal, an absolute path, a
    // Symlink entry, and a CharacterDevice entry. Each must be denied.
    let adversarial = [
        (
            "parent traversal",
            ArchiveEntry::new("../escape", ArchiveEntryKind::File),
        ),
        (
            "absolute path",
            ArchiveEntry::new("/abs/path", ArchiveEntryKind::File),
        ),
        (
            "symlink",
            ArchiveEntry::new("src/link", ArchiveEntryKind::Symlink),
        ),
        (
            "character device",
            ArchiveEntry::new("dev/tty", ArchiveEntryKind::CharacterDevice),
        ),
    ];

    for (label, entry) in adversarial {
        let err = validate_import_archive(&[entry], &destination, &allowed_roots)
            .expect_err(&format!("adversarial entry ({label}) must be rejected"));
        assert_eq!(
            err.reason, "workcell_tar_path_denied",
            "adversarial entry ({label}) must be denied with tar_path_denied",
        );
    }
}

#[test]
fn jailgun_adversarial_entries_do_not_smuggle_in_with_a_clean_sibling() {
    // Even mixed into an otherwise-clean batch, a single adversarial entry must
    // fail the whole import.
    let allowed_roots = vec![repo_root()];
    let destination = repo_root();

    let mut batch = clean_tree();
    batch.push(ArchiveEntry::new(
        "../../etc/passwd",
        ArchiveEntryKind::File,
    ));

    let err = validate_import_archive(&batch, &destination, &allowed_roots)
        .expect_err("a clean batch with one traversal entry must be rejected");
    assert_eq!(err.reason, "workcell_tar_path_denied");
}

#[test]
fn jailgun_export_outside_approved_roots_is_denied() {
    let allowed_roots = vec![repo_root()];
    let err = validate_export_paths(
        &[PathBuf::from("/workspace/core/api/src/lib.rs")],
        &allowed_roots,
    )
    .expect_err("export outside the approved repo roots must be denied");
    assert_eq!(err.reason, "workcell_tar_path_denied");
}
