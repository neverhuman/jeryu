use std::fs;
use std::os::unix::fs::PermissionsExt;

/// A physical private parent suitable for persistent writer-custody tests.
pub fn private_directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
