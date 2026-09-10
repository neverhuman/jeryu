use super::*;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::process::Command;

fn fixture(test: impl FnOnce(&Path)) {
    let root = tempfile::Builder::new()
        .prefix("jeryu-census-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
        .keep();
    eprintln!("census fixture retained on failure: {}", root.display());
    let before = fs::symlink_metadata(&root).unwrap();
    test(&root);
    let after = fs::symlink_metadata(&root).unwrap();
    assert_eq!(
        (before.dev(), before.ino(), before.uid(), before.mode()),
        (after.dev(), after.ino(), after.uid(), after.mode())
    );
    assert_eq!(root.canonicalize().unwrap(), root);
    for row in fs::read_to_string("/proc/self/mountinfo").unwrap().lines() {
        let mount = row.split_whitespace().nth(4).unwrap();
        assert!(
            mount != root.to_str().unwrap() && !mount.starts_with(&format!("{}/", root.display()))
        );
    }
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        assert!(matches!(
            entry.file_name().to_str(),
            Some("report" | "link" | "fifo")
        ));
        assert!(!fs::symlink_metadata(entry.path()).unwrap().is_dir());
        fs::remove_file(entry.path()).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[test]
fn report_custody_refuses_symlink_hardlink_fifo_and_oversize_without_blocking() {
    fixture(|root| {
        let report = root.join("report");
        let link = root.join("link");
        let fifo = root.join("fifo");
        fs::write(&report, b"report bytes").unwrap();
        assert_eq!(read_report(&report).unwrap(), b"report bytes");
        assert!(hold_binary(&report).is_ok());
        symlink(&report, &link).unwrap();
        assert!(read_report(&link).is_err());
        assert!(hold_binary(&link).is_err());
        assert_eq!(fs::read(&report).unwrap(), b"report bytes");
        fs::remove_file(&link).unwrap();
        fs::hard_link(&report, &link).unwrap();
        assert!(read_report(&report).is_err());
        assert!(hold_binary(&report).is_err());
        fs::remove_file(&link).unwrap();
        assert!(
            Command::new("/usr/bin/mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(read_report(&fifo).is_err());
        assert!(hold_binary(&fifo).is_err());
        fs::OpenOptions::new()
            .write(true)
            .open(&report)
            .unwrap()
            .set_len(32 * 1024 * 1024 + 1)
            .unwrap();
        assert!(read_report(&report).is_err());
        assert!(hold_binary(&report).is_ok());
        fs::OpenOptions::new()
            .write(true)
            .open(&report)
            .unwrap()
            .set_len(128 * 1024 * 1024 + 1)
            .unwrap();
        assert!(hold_binary(&report).is_err());
        assert!(read_report(&root.join("absent")).is_err());
    });
}

#[test]
fn source_paths_require_physical_custody_and_external_commit() {
    fixture(|root| {
        let mut source = Source {
            repository: "neverhuman/jeryu".into(),
            scope: "dependency".into(),
            path: Some(".".into()),
            commit: None,
            minimum: 85,
            required: true,
            reason: None,
        };
        assert!(source_path(root, &source).is_err());
        source.commit = Some("a".repeat(40));
        assert_eq!(source_path(root, &source).unwrap(), root);
        for path in ["../outside", "/outside", "missing"] {
            source.path = Some(path.into());
            assert!(source_path(root, &source).is_err());
        }
        symlink(root, root.join("link")).unwrap();
        source.path = Some("link".into());
        assert!(source_path(root, &source).is_err());
    });
}

#[test]
fn stronger_component_floors_are_preserved() {
    for component in ["jeryu-cache", "jeryu-jira", "jeryu-ci-runner"] {
        assert_eq!(effective_floor(component), 91);
    }
    for component in [
        "jeryu",
        "jeryu-tool",
        "jeryu-tool-finder",
        "jeryu-intelligence",
    ] {
        assert_eq!(effective_floor(component), 85);
    }
}
