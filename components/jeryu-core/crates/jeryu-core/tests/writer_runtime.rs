#![cfg(unix)]

mod support;

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use jeryu_core::{CreateUserRequest, ForgeCore, ForgeError, MutationCoordinator};
use support::private_directory;
use uuid::Uuid;

fn create_user(core: &ForgeCore, login: &str) {
    core.create_user(CreateUserRequest {
        login: login.to_string(),
        name: None,
        email: None,
    })
    .unwrap();
}

fn private_file(path: &Path, bytes: &[u8]) -> File {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file
}

fn denied(result: Result<ForgeCore, ForgeError>) {
    assert!(
        matches!(result, Err(ForgeError::WriterUnavailable(_))),
        "writer custody must fail before opening SQLite: {result:?}"
    );
}

#[test]
fn separately_opened_handles_share_live_state_and_keep_each_others_updates() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let root = temp.path().join("git");
    let first = ForgeCore::open_managed(&db, &root).unwrap();
    let second = ForgeCore::open_managed(&db, &root).unwrap();
    assert!(std::ptr::eq(first.coordinator(), second.coordinator()));
    create_user(&first, "alice");
    assert!(second.get_user("alice").is_ok());
    create_user(&second, "bob");
    assert!(first.get_user("bob").is_ok());
    drop(first);
    denied(ForgeCore::open_sqlite(&db));
    drop(second);

    // No strong runtime reference remains; this is a database reload.
    let reopened = ForgeCore::open_managed(&db, &root).unwrap();
    assert!(reopened.get_user("alice").is_ok());
    assert!(reopened.get_user("bob").is_ok());
}

#[test]
fn simultaneous_openers_share_one_runtime_without_overwriting_other_users() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let root = temp.path().join("git");
    let barrier = Arc::new(Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|index| {
            let (db, root, barrier) = (db.clone(), root.clone(), Arc::clone(&barrier));
            std::thread::spawn(move || {
                barrier.wait();
                let core = ForgeCore::open_managed(db, root).unwrap();
                create_user(&core, &format!("user-{index}"));
                core
            })
        })
        .collect();
    let handles: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    for handle in &handles {
        assert!(std::ptr::eq(handles[0].coordinator(), handle.coordinator()));
        for index in 0..4 {
            assert!(handle.get_user(&format!("user-{index}")).is_ok());
        }
    }
    drop(handles);
    let reloaded = ForgeCore::open_managed(db, root).unwrap();
    for index in 0..4 {
        assert!(reloaded.get_user(&format!("user-{index}")).is_ok());
    }
}

#[test]
fn one_database_or_storage_root_cannot_join_two_active_pairs() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let other_db = temp.path().join("other.sqlite");
    let root = temp.path().join("git");
    let other_root = temp.path().join("other-git");
    let core = ForgeCore::open_managed(&db, &root).unwrap();
    denied(ForgeCore::open_managed(&db, &other_root));
    denied(ForgeCore::open_managed(&other_db, &root));
    assert!(
        !other_db.exists(),
        "conflicting open must not migrate another database"
    );
    denied(ForgeCore::open_sqlite(&db));
    drop(core);
    let database_only = ForgeCore::open_sqlite(&db).unwrap();
    denied(ForgeCore::open_managed(&db, &root));
    drop(database_only);
    assert!(ForgeCore::open_managed(&other_db, &root).is_ok());
}

#[test]
fn independently_backed_runtimes_and_new_in_memory_stores_remain_separate() {
    let temp = private_directory();
    let first = ForgeCore::open_sqlite(temp.path().join("first.sqlite")).unwrap();
    let second = ForgeCore::open_sqlite(temp.path().join("second.sqlite")).unwrap();
    create_user(&first, "alice");
    assert!(second.get_user("alice").is_err());
    let memory = ForgeCore::new();
    create_user(&memory, "bob");
    assert!(memory.clone().get_user("bob").is_ok());
    assert!(ForgeCore::new().get_user("bob").is_err());
}

fn run_process_probe(db: &Path, root: Option<&Path>, expectation: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "writer_process_probe", "--nocapture"])
        .env("JERYU_TEST_WRITER_DATABASE", db)
        .env("JERYU_TEST_WRITER_EXPECTATION", expectation)
        .env_remove("JERYU_TEST_WRITER_ROOT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(root) = root {
        command.env("JERYU_TEST_WRITER_ROOT", root);
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("writer probe did not finish within its bound: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "writer process probe: {output:?}");
}

#[test]
fn writer_process_probe() {
    let Some(db) = std::env::var_os("JERYU_TEST_WRITER_DATABASE") else {
        return;
    };
    let result = match std::env::var_os("JERYU_TEST_WRITER_ROOT") {
        Some(root) => ForgeCore::open_managed(db, root),
        None => ForgeCore::open_sqlite(db),
    };
    match std::env::var("JERYU_TEST_WRITER_EXPECTATION")
        .unwrap()
        .as_str()
    {
        "deny" => denied(result),
        "allow" => create_user(&result.unwrap(), "child"),
        other => panic!("unknown writer probe expectation {other}"),
    }
}

#[test]
fn a_second_process_is_denied_until_the_last_handle_releases_both_resources() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let other_db = temp.path().join("other.sqlite");
    let root = temp.path().join("git");
    let first = ForgeCore::open_managed(&db, &root).unwrap();
    let second = ForgeCore::open_managed(&db, &root).unwrap();
    create_user(&first, "parent");
    run_process_probe(&db, Some(&root), "deny");
    run_process_probe(&other_db, Some(&root), "deny");
    assert!(!other_db.exists());
    drop(first);
    run_process_probe(&db, None, "deny");
    drop(second);
    run_process_probe(&db, Some(&root), "allow");
    let reopened = ForgeCore::open_managed(&db, &root).unwrap();
    assert!(reopened.get_user("parent").is_ok());
    assert!(reopened.get_user("child").is_ok());
}

#[test]
fn database_inode_lease_is_acquired_before_sqlite_opens_or_migrates() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let original = b"not an SQLite database; writer refusal must precede parsing";
    let file = private_file(&db, original);
    file.try_lock().unwrap();
    run_process_probe(&db, None, "deny");
    assert_eq!(fs::read(&db).unwrap(), original);
}

#[test]
fn failed_open_releases_all_leases_and_does_not_intern_a_partial_runtime() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let root = temp.path().join("git");
    drop(private_file(&db, b"invalid database"));
    let error = ForgeCore::open_managed(&db, &root).unwrap_err();
    assert!(matches!(error, ForgeError::Storage(_)), "{error:?}");
    drop(private_file(&db, b""));
    let core = ForgeCore::open_managed(&db, &root).unwrap();
    create_user(&core, "recovered");
    drop(core);
    assert!(
        ForgeCore::open_managed(db, root)
            .unwrap()
            .get_user("recovered")
            .is_ok()
    );
}

#[test]
fn database_rename_fences_old_runtime_and_retains_its_inode_lease() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let moved = temp.path().join("moved.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    create_user(&core, "before");
    fs::rename(&db, &moved).unwrap();
    let result = core.create_user(CreateUserRequest {
        login: "after".into(),
        name: None,
        email: None,
    });
    assert!(matches!(result, Err(ForgeError::WriterUnavailable(_))));
    assert!(
        core.get_user("after").is_err(),
        "failed persistence must restore shared state"
    );
    denied(ForgeCore::open_sqlite(&moved));
    drop(core);
    let reopened = ForgeCore::open_sqlite(&moved).unwrap();
    assert!(reopened.get_user("before").is_ok());
    assert!(reopened.get_user("after").is_err());
}

#[test]
fn replaced_database_and_storage_inodes_do_not_reuse_cached_runtime() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    let root = temp.path().join("git");
    let core = ForgeCore::open_managed(&db, &root).unwrap();
    fs::rename(&db, temp.path().join("original.sqlite")).unwrap();
    drop(private_file(&db, b""));
    denied(ForgeCore::open_managed(&db, &root));
    drop(core);
    let core = ForgeCore::open_managed(&db, &root).unwrap();
    fs::rename(&root, temp.path().join("original-git")).unwrap();
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    denied(ForgeCore::open_managed(&db, &root));
    drop(core);
}

#[test]
fn changing_an_ancestor_to_a_symlink_fences_the_same_leaf_inode() {
    let temp = private_directory();
    let parent = temp.path().join("parent");
    let moved = temp.path().join("moved");
    let db = parent.join("nested/forge.sqlite");
    let core = ForgeCore::open_sqlite(&db).unwrap();
    fs::rename(&parent, &moved).unwrap();
    symlink(&moved, &parent).unwrap();
    let result = core.create_user(CreateUserRequest {
        login: "rejected".into(),
        name: None,
        email: None,
    });
    assert!(matches!(result, Err(ForgeError::WriterUnavailable(_))));
    assert!(core.get_user("rejected").is_err());
    denied(ForgeCore::open_sqlite(&db));
}

#[test]
fn symlinks_hardlinks_and_reserved_lease_paths_are_rejected_without_sqlite_writes() {
    let temp = private_directory();
    let db = temp.path().join("forge.sqlite");
    drop(private_file(&db, b"preserved"));
    let alias = temp.path().join("alias.sqlite");
    symlink(&db, &alias).unwrap();
    denied(ForgeCore::open_sqlite(&alias));
    fs::remove_file(&alias).unwrap();
    fs::hard_link(&db, &alias).unwrap();
    denied(ForgeCore::open_sqlite(&db));
    assert_eq!(fs::read(&db).unwrap(), b"preserved");
    denied(ForgeCore::open_sqlite(
        temp.path().join("forge.sqlite.writer.lock"),
    ));
    denied(ForgeCore::open_sqlite(
        temp.path().join(".jeryu-writer.lock"),
    ));
}

#[test]
fn newly_created_directories_are_private_and_existing_writable_ones_are_rejected() {
    let temp = private_directory();
    let parent = temp.path().join("new/nested");
    let root = temp.path().join("git");
    let core = ForgeCore::open_managed(parent.join("forge.sqlite"), &root).unwrap();
    assert_eq!(
        fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    drop(core);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o770)).unwrap();
    denied(ForgeCore::open_managed(parent.join("forge.sqlite"), &root));
}

#[test]
fn coordinator_sorts_and_deduplicates_repository_locks_across_callers() {
    let coordinator = Arc::new(MutationCoordinator::default());
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    let barrier = Arc::new(Barrier::new(2));
    let (tx, rx) = std::sync::mpsc::channel();
    for ids in [[first, second, first], [second, first, second]] {
        let (coordinator, barrier, tx) =
            (Arc::clone(&coordinator), Arc::clone(&barrier), tx.clone());
        std::thread::spawn(move || {
            barrier.wait();
            coordinator
                .with_repositories(&ids, || {
                    tx.send(()).unwrap();
                    Ok(())
                })
                .unwrap();
        });
    }
    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
}
