//! Integration tests for smart-HTTP authentication and authorization.
//!
//! These exercise the public [`SmartHttpServer::route`] entry point with
//! synthetic [`HttpRequest`] values so the auth decisions are tested without a
//! live socket. Cases that pass the auth gate and reach the Git pack process
//! are guarded on `git` availability.

mod common;

use jeryu_gitd::auth::{AuthRegistry, SCOPE_READ, SCOPE_WRITE};
use jeryu_gitd::smart_http::{HttpRequest, SmartHttpServer};
use jeryu_gitd::{GitdConfig, RepoId, RepoManager};
use std::collections::HashMap;
use std::path::Path;

fn server_for(root: &Path) -> SmartHttpServer {
    SmartHttpServer::new(RepoManager::new(GitdConfig::new(root)))
}

fn basic_header(user: &str, pass: &str) -> String {
    // Standard base64 of "user:pass".
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let raw = format!("{user}:{pass}");
    let bytes = raw.as_bytes();
    let mut enc = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        enc.push(TABLE[(b0 >> 2) as usize] as char);
        enc.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        enc.push(if chunk.len() > 1 {
            TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        enc.push(if chunk.len() > 2 {
            TABLE[(b2 & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    format!("Basic {enc}")
}

fn post_receive_pack(owner: &str, authorization: Option<&str>, is_loopback: bool) -> HttpRequest {
    let mut headers = HashMap::new();
    if let Some(value) = authorization {
        headers.insert("authorization".to_string(), value.to_string());
    }
    HttpRequest {
        method: "POST".to_string(),
        path: format!("/{owner}/demo.git/git-receive-pack"),
        query: HashMap::new(),
        headers,
        body: Vec::new(),
        is_loopback,
    }
}

fn get_info_refs(
    owner: &str,
    service: &str,
    authorization: Option<&str>,
    is_loopback: bool,
) -> HttpRequest {
    let mut headers = HashMap::new();
    if let Some(value) = authorization {
        headers.insert("authorization".to_string(), value.to_string());
    }
    let mut query = HashMap::new();
    query.insert("service".to_string(), service.to_string());
    HttpRequest {
        method: "GET".to_string(),
        path: format!("/{owner}/demo.git/info/refs"),
        query,
        headers,
        body: Vec::new(),
        is_loopback,
    }
}

#[test]
fn unauthenticated_non_loopback_receive_pack_is_401() {
    let root = common::temp_dir("jeryu-http-401");
    let server = server_for(&root);
    let resp = server.route(post_receive_pack("acme", None, false));
    assert_eq!(resp.status(), 401);
    // GitHub-shaped challenge.
    assert_eq!(
        resp.header("WWW-Authenticate"),
        Some("Basic realm=\"jeryu\"")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unauthenticated_non_loopback_info_refs_receive_is_401() {
    let root = common::temp_dir("jeryu-http-inforefs-401");
    let server = server_for(&root);
    let resp = server.route(get_info_refs("acme", "git-receive-pack", None, false));
    assert_eq!(resp.status(), 401);
    assert_eq!(
        resp.header("WWW-Authenticate"),
        Some("Basic realm=\"jeryu\"")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn valid_token_wrong_owner_write_is_403() {
    let root = common::temp_dir("jeryu-http-403");
    // Mint a token for alice with read+write.
    let token = {
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("registry open: {e}"));
        reg.mint_pat(
            "alice",
            vec![SCOPE_READ.to_string(), SCOPE_WRITE.to_string()],
        )
        .unwrap_or_else(|e| panic!("mint: {e}"))
        .token()
    };
    let server = server_for(&root);
    // alice pushes into bob's namespace -> 403 JSON.
    let header = basic_header("x-access-token", &token);
    let resp = server.route(post_receive_pack("bob", Some(&header), false));
    assert_eq!(resp.status(), 403);
    let body = String::from_utf8_lossy(resp.body());
    assert!(
        body.contains("\"message\""),
        "expected JSON body, got {body}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn presented_invalid_token_no_loopback_bypass_is_401() {
    let root = common::temp_dir("jeryu-http-nobypass");
    let server = server_for(&root);
    // Even on loopback, a presented-but-invalid credential is rejected.
    let header = basic_header("x-access-token", "jpat_bogus_token");
    let resp = server.route(post_receive_pack("acme", Some(&header), true));
    assert_eq!(resp.status(), 401);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn loopback_no_cred_upload_pack_advertisement_is_allowed() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-http-loopback-read");
    let manager = RepoManager::new(GitdConfig::new(&root));
    manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|e| panic!("id: {e}")))
        .unwrap_or_else(|e| panic!("create: {e}"));
    let server = server_for(&root);
    // Loopback, no credential, read advertisement -> passes auth and reaches Git.
    let resp = server.route(get_info_refs("acme", "git-upload-pack", None, true));
    assert_eq!(
        resp.status(),
        200,
        "loopback no-cred upload-pack advertisement should be allowed"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn loopback_no_cred_receive_pack_advertisement_is_allowed() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-http-loopback-write");
    let manager = RepoManager::new(GitdConfig::new(&root));
    manager
        .create_bare(&RepoId::new("acme", "demo").unwrap_or_else(|e| panic!("id: {e}")))
        .unwrap_or_else(|e| panic!("create: {e}"));
    let server = server_for(&root);
    // Loopback local operator may even prepare a push (receive-pack advert).
    let resp = server.route(get_info_refs("acme", "git-receive-pack", None, true));
    assert_eq!(resp.status(), 200);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn owner_token_write_to_own_namespace_is_allowed() {
    if !common::git_available() {
        return;
    }
    let root = common::temp_dir("jeryu-http-owner-allowed");
    let manager = RepoManager::new(GitdConfig::new(&root));
    manager
        .create_bare(&RepoId::new("alice", "demo").unwrap_or_else(|e| panic!("id: {e}")))
        .unwrap_or_else(|e| panic!("create: {e}"));
    let token = {
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("registry open: {e}"));
        reg.mint_pat(
            "alice",
            vec![SCOPE_WRITE.to_string(), SCOPE_READ.to_string()],
        )
        .unwrap_or_else(|e| panic!("mint: {e}"))
        .token()
    };
    let server = server_for(&root);
    // Bearer form, non-loopback, into her own namespace -> reaches Git (200).
    let header = format!("Bearer {token}");
    let resp = server.route(get_info_refs(
        "alice",
        "git-receive-pack",
        Some(&header),
        false,
    ));
    assert_eq!(resp.status(), 200);
    let _ = std::fs::remove_dir_all(root);
}
