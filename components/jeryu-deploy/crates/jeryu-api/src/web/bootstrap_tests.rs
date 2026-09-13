//! First-start credential custody and idempotence regression.

use super::*;

#[test]
fn bootstrap_resumes_after_receipt_commit_without_replacing_credentials() {
    let state = WebState::new(ForgeCore::new());
    let directory = tempfile::tempdir().unwrap();
    let password = bootstrap::prepare_bootstrap_password(&state, directory.path()).unwrap();
    assert!(state.core.get_account("jeryu-admin").is_err());
    let path = directory.path().join("bootstrap-credentials.json");
    let receipt = std::fs::read(&path).unwrap();
    // A new server process has no in-memory account from the first attempt.
    let restarted = WebState::new(ForgeCore::new());
    bootstrap_public_accounts_with_admin_password(&restarted, directory.path(), None).unwrap();
    let account = restarted
        .core
        .authenticate_password("jeryu-admin", &password)
        .unwrap();
    assert_eq!(account.role, UserRole::Admin);
    assert!(account.must_change_password);
    assert!(std::fs::read(&path).unwrap() == receipt);
    bootstrap_public_accounts_with_admin_password(&restarted, directory.path(), None).unwrap();
    assert!(std::fs::read(&path).unwrap() == receipt);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn invalid_or_unwritable_receipt_cannot_create_an_unrecoverable_account() {
    use std::os::unix::fs::PermissionsExt;
    for bytes in [b"{\"credentials\":".as_slice(), b"[]", b"{}", b"null"] {
        let state = WebState::new(ForgeCore::new());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bootstrap-credentials.json");
        std::fs::write(&path, bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            bootstrap_public_accounts_with_admin_password(&state, directory.path(), None).is_err()
        );
        assert!(state.core.get_account("jeryu-admin").is_err());
        assert!(std::fs::read(&path).unwrap() == bytes);
    }
    let state = WebState::new(ForgeCore::new());
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("bootstrap-credentials.json")).unwrap();
    assert!(bootstrap_public_accounts_with_admin_password(&state, directory.path(), None).is_err());
    assert!(state.core.get_account("jeryu-admin").is_err());
}

#[test]
fn interrupted_bootstrap_refuses_unsafe_receipt_custody() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for variant in ["symlink", "hardlink", "public", "fifo"] {
        let state = WebState::new(ForgeCore::new());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bootstrap-credentials.json");
        let held = directory.path().join("held-receipt");
        if variant == "fifo" {
            rustix::fs::mkfifoat(
                rustix::fs::CWD,
                &path,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            )
            .unwrap();
        } else {
            bootstrap::prepare_bootstrap_password(&state, directory.path()).unwrap();
            match variant {
                "symlink" => {
                    std::fs::rename(&path, &held).unwrap();
                    symlink(&held, &path).unwrap();
                }
                "hardlink" => std::fs::hard_link(&path, &held).unwrap(),
                "public" => {
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap()
                }
                _ => unreachable!(),
            }
        }
        assert!(
            bootstrap_public_accounts_with_admin_password(&state, directory.path(), None).is_err(),
            "{variant}"
        );
        assert!(state.core.get_account("jeryu-admin").is_err(), "{variant}");
        assert!(std::fs::symlink_metadata(&path).is_ok(), "{variant}");
    }
}

#[test]
fn bootstrap_creates_only_an_admin_and_does_not_replace_the_one_time_receipt() {
    let state = WebState::new(ForgeCore::new());
    let directory = tempfile::tempdir().unwrap();
    bootstrap_public_accounts_with_admin_password(&state, directory.path(), None).unwrap();
    let path = directory.path().join("bootstrap-credentials.json");
    let original = std::fs::read(&path).unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&original).unwrap();
    assert_eq!(receipt["credentials"].as_array().unwrap().len(), 1);
    assert_eq!(receipt["credentials"][0]["login"], "jeryu-admin");
    let account = state.core.get_account("jeryu-admin").unwrap();
    assert_eq!(account.role, UserRole::Admin);
    assert!(account.must_change_password);
    assert!(state.core.get_account("jordanh").is_err());
    assert!(state.core.get_account("jepsont").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    bootstrap_public_accounts_with_admin_password(&state, directory.path(), None).unwrap();
    assert!(std::fs::read(path).unwrap() == original);
}
