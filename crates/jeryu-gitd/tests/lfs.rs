use jeryu_gitd::hash::sha256_hex;
use jeryu_gitd::lfs::LfsStore;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn lfs_stores_by_sha256() {
    let root = std::env::temp_dir().join(format!(
        "jeryu-lfs-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let store = LfsStore::new(&root);
    let data = b"phase1-lfs";
    let oid = sha256_hex(data);
    store
        .put_bytes(&oid, data)
        .unwrap_or_else(|err| panic!("put failed: {err}"));
    assert!(store.exists(&oid));
    assert_eq!(
        store
            .get(&oid)
            .unwrap_or_else(|err| panic!("get failed: {err}")),
        data
    );
    let _ = std::fs::remove_dir_all(root);
}
