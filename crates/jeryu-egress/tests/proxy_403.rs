//! End-to-end test: a CONNECT to a non-allowlisted host is rejected with 403
//! and NO upstream connection is ever attempted.
//!
//! This binds an ephemeral loopback listener (`127.0.0.1:0`) and speaks raw
//! HTTP to it, so it needs no real DNS or outbound network — the proxy denies
//! the request before it would resolve or connect anywhere.

use jeryu_egress::{Allowlist, Budget, Proxy};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Spawn the proxy on an ephemeral loopback port and return its bound address.
///
/// We grab a free ephemeral port, drop the probe listener, then let the proxy's
/// own public `serve` loop rebind it. The 100ms sleep lets that rebind settle.
async fn spawn_proxy(proxy: Proxy) -> SocketAddr {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    tokio::spawn(async move {
        let _ = proxy.serve(addr).await;
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    addr
}

#[tokio::test]
async fn connect_to_non_allowlisted_host_returns_403() {
    let proxy = Proxy::new(Allowlist::default(), Budget::new());
    let addr = spawn_proxy(proxy).await;

    let mut client = TcpStream::connect(addr).await.unwrap();
    client
        .write_all(b"CONNECT evil.example.com:443 HTTP/1.1\r\nHost: evil.example.com:443\r\n\r\n")
        .await
        .unwrap();
    client.flush().await.unwrap();

    let mut buf = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut buf))
        .await
        .expect("proxy should respond promptly");
    read.unwrap();
    let response = String::from_utf8_lossy(&buf);
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "expected 403 for non-allowlisted host, got: {response:?}"
    );
}

#[tokio::test]
async fn budget_kill_switch_denies_allowlisted_host() {
    let budget = Budget::new();
    budget.trip(); // blow the budget BEFORE serving
    let proxy = Proxy::new(Allowlist::default(), budget);
    let addr = spawn_proxy(proxy).await;

    let mut client = TcpStream::connect(addr).await.unwrap();
    // github.com IS allowlisted, but the budget kill switch must still deny it.
    client
        .write_all(b"CONNECT github.com:443 HTTP/1.1\r\nHost: github.com:443\r\n\r\n")
        .await
        .unwrap();
    client.flush().await.unwrap();

    let mut buf = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(5), client.read_to_end(&mut buf))
        .await
        .expect("proxy should respond promptly");
    read.unwrap();
    let response = String::from_utf8_lossy(&buf);
    assert!(
        response.starts_with("HTTP/1.1 403"),
        "expected 403 when budget exceeded, got: {response:?}"
    );
}
