use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

pub fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("jeryu_mirror-{name}-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}
