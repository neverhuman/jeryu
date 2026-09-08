//! First-start credential custody and idempotence regression.

use super::*;

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
