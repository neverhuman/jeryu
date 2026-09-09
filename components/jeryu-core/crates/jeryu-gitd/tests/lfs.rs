mod common;

use jeryu_gitd::hash::sha256_hex;
use jeryu_gitd::lfs::LfsStore;
use jeryu_gitd::smart_http::{HttpRequest, SmartHttpServer};
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Cursor;
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

#[test]
fn lfs_batch_upload_actions_and_existing_dedupe() {
    let root = temp_root("jeryu-lfs-batch-upload");
    let store = LfsStore::new(&root);
    let data = b"model-v1";
    let oid = sha256_hex(data);
    let request = serde_json::json!({
        "operation": "upload",
        "transfers": ["basic"],
        "hash_algo": "sha256",
        "objects": [{ "oid": oid, "size": data.len() }]
    })
    .to_string();

    let response = batch_json(&store, &request);
    let object = &response["objects"][0];
    assert!(
        object["actions"]["upload"]["href"]
            .as_str()
            .unwrap()
            .ends_with(&oid)
    );
    assert!(
        object["actions"]["verify"]["href"]
            .as_str()
            .unwrap()
            .ends_with(&format!("{oid}/verify"))
    );

    store.put_bytes(&oid, data).unwrap();
    let response = batch_json(&store, &request);
    assert!(
        response["objects"][0]["actions"]["upload"].is_null(),
        "existing object should not request another upload: {response}"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lfs_batch_download_reports_missing_then_download_action() {
    let root = temp_root("jeryu-lfs-batch-download");
    let store = LfsStore::new(&root);
    let data = b"model-v2";
    let oid = sha256_hex(data);
    let request = serde_json::json!({
        "operation": "download",
        "objects": [{ "oid": oid, "size": data.len() }]
    })
    .to_string();

    let missing = batch_json(&store, &request);
    assert_eq!(missing["objects"][0]["error"]["code"], 404);

    store.put_bytes(&oid, data).unwrap();
    let present = batch_json(&store, &request);
    assert!(
        present["objects"][0]["actions"]["download"]["href"]
            .as_str()
            .unwrap()
            .ends_with(&oid)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lfs_batch_rejects_invalid_oid_and_unsupported_hash() {
    let root = temp_root("jeryu-lfs-batch-invalid");
    let store = LfsStore::new(&root);
    let invalid_oid = serde_json::json!({
        "operation": "upload",
        "objects": [{ "oid": "not-a-sha", "size": 1 }]
    })
    .to_string();
    assert!(
        store
            .batch_response(&invalid_oid, objects_url(), None, 1024)
            .unwrap_err()
            .to_string()
            .contains("LFS oid")
    );

    let unsupported_hash = serde_json::json!({
        "operation": "upload",
        "hash_algo": "sha512",
        "objects": []
    })
    .to_string();
    assert!(
        store
            .batch_response(&unsupported_hash, objects_url(), None, 1024)
            .unwrap_err()
            .to_string()
            .contains("only sha256")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lfs_streaming_put_rejects_wrong_size_and_wrong_sha() {
    let root = temp_root("jeryu-lfs-streaming");
    let store = LfsStore::new(&root);
    let data = b"streamed-model";
    let oid = sha256_hex(data);

    assert!(
        store
            .put_reader_with_limit(&oid, Some(data.len() as u64 + 1), 1024, Cursor::new(data))
            .unwrap_err()
            .to_string()
            .contains("size mismatch")
    );
    assert!(
        store
            .put_reader_with_limit(
                &"0".repeat(64),
                Some(data.len() as u64),
                1024,
                Cursor::new(data)
            )
            .unwrap_err()
            .to_string()
            .contains("sha256 mismatch")
    );

    store
        .put_reader_with_limit(&oid, Some(data.len() as u64), 1024, Cursor::new(data))
        .unwrap();
    store.verify(&oid, data.len() as u64).unwrap();
    assert!(store.verify(&oid, data.len() as u64 + 1).is_err());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lfs_locks_verify_route_returns_empty_lock_sets() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-lfs-locks-verify");
    let manager = RepoManager::new(GitdConfig::new(&root));
    manager
        .create_bare(
            &RepoId::new("acme", "demo").unwrap_or_else(|err| panic!("repo id failed: {err}")),
        )
        .unwrap_or_else(|err| panic!("create bare repo: {err}"));
    let server = SmartHttpServer::new(manager);
    let response = server.route(HttpRequest {
        method: "POST".to_string(),
        path: "/git/acme/demo.git/info/lfs/locks/verify".to_string(),
        query: HashMap::new(),
        headers: HashMap::new(),
        body: b"{}".to_vec(),
        is_loopback: true,
        auth_prechecked: false,
    });

    assert_eq!(response.status(), 200);
    assert_eq!(response.content_type(), "application/vnd.git-lfs+json");
    let body: Value = serde_json::from_slice(response.body()).unwrap();
    assert_eq!(body["ours"].as_array().unwrap().len(), 0);
    assert_eq!(body["theirs"].as_array().unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(root);
}

fn batch_json(store: &LfsStore, request: &str) -> Value {
    serde_json::from_slice(
        &store
            .batch_response(request, objects_url(), None, 1024)
            .unwrap(),
    )
    .unwrap()
}

fn objects_url() -> &'static str {
    "http://127.0.0.1/git/acme/demo.git/info/lfs/objects"
}

fn temp_root(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{}-{}",
        prefix,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}
