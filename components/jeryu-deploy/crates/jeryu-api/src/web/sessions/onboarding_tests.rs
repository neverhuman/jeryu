use super::ensure_claude_onboarding_state;
use std::fs::{self, File, Metadata};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

// Keep failed fixtures. Successful tests remove only inspected, single-link
// files or empty directories; no recursive cleanup follows a substituted link.
fn with_state_fixture(test: impl FnOnce(&Path)) {
    let root = tempfile::Builder::new()
        .prefix("jeryu-onboarding-state-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("state fixture")
        .keep();
    let held = File::open(&root).expect("hold fixture root");
    let original = held.metadata().expect("fixture metadata");
    let state = root.join("claude.json");
    test(&state);
    let current = fs::symlink_metadata(&root).expect("fixture root remains");
    assert!(current.is_dir() && !current.file_type().is_symlink());
    assert_eq!(fs::canonicalize(&root).unwrap(), root);
    assert_eq!(identity(&current), identity(&original));
    assert_eq!(identity(&held.metadata().unwrap()), identity(&original));
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(entry.path(), state, "unexpected fixture entry; retain root");
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(metadata.dev(), original.dev());
        assert_eq!(metadata.uid(), original.uid());
        assert_eq!(metadata.gid(), original.gid());
        if metadata.is_file() {
            assert_eq!(metadata.nlink(), 1);
            fs::remove_file(entry.path()).unwrap();
        } else {
            assert!(metadata.is_dir());
            // Refuses nonempty directories and mountpoints without descending.
            fs::remove_dir(entry.path()).unwrap();
        }
    }
    fs::remove_dir(&root).unwrap();
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

#[test]
fn onboarding_initializes_only_missing_state() {
    with_state_fixture(|path| {
        ensure_claude_onboarding_state(path, "claude").unwrap();
        let state: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert_eq!(state["numStartups"], 1);
        assert_eq!(state["autoUpdates"], false);
        assert_eq!(state["theme"], "dark");
        assert_eq!(state["lastOnboardingVersion"], "2.1.170");
        assert_eq!(state["hasSeenAutoDefaultNudge"], true);
        assert_eq!(state["hasSeenAutoDefaultNotice"], true);
        assert_eq!(fs::metadata(path).unwrap().mode() & 0o777, 0o600);
    });
}

#[test]
fn onboarding_preserves_existing_object_fields() {
    with_state_fixture(|path| {
        fs::write(
            path,
            br#"{"hasCompletedOnboarding":false,"theme":"light","numStartups":9,"oauthAccount":{"fixture":true}}"#,
        )
        .unwrap();
        ensure_claude_onboarding_state(path, "claude").unwrap();
        let state: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert_eq!(state["theme"], "light");
        assert_eq!(state["numStartups"], 9);
        assert_eq!(state["oauthAccount"], serde_json::json!({"fixture": true}));
    });
}

#[test]
fn onboarding_rejects_invalid_state_without_mutation() {
    for bytes in [
        b"{broken".as_slice(),
        b"[]",
        b"null",
        b"true",
        b"17",
        br#""text""#,
        b"\xff",
    ] {
        with_state_fixture(|path| {
            fs::write(path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o640)).unwrap();
            let before = fs::symlink_metadata(path).unwrap();
            let error = ensure_claude_onboarding_state(path, "claude").unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("Claude onboarding state"));
            assert!(error.to_string().contains(&path.display().to_string()));
            assert_eq!(fs::read(path).unwrap(), bytes);
            let after = fs::symlink_metadata(path).unwrap();
            assert_eq!(identity(&after), identity(&before));
            assert_eq!(
                (after.mtime(), after.mtime_nsec()),
                (before.mtime(), before.mtime_nsec())
            );
        });
    }
}

#[test]
fn onboarding_reports_read_failure_without_mutation() {
    with_state_fixture(|path| {
        // A directory produces a real read error even when tests run as root.
        // A chmod-only PermissionDenied fixture would be bypassed by root.
        fs::create_dir(path).unwrap();
        let before = fs::symlink_metadata(path).unwrap();
        let error = ensure_claude_onboarding_state(path, "claude").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("failed to read Claude onboarding state")
        );
        assert!(error.to_string().contains(&path.display().to_string()));
        assert_eq!(
            identity(&fs::symlink_metadata(path).unwrap()),
            identity(&before)
        );
        assert!(fs::read_dir(path).unwrap().next().is_none());
    });
}
