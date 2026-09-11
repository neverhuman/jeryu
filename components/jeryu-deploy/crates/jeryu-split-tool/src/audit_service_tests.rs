use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

async fn serve(fixture: &Fixture) -> (SocketAddr, oneshot::Sender<()>, JoinHandle<()>) {
    let route = fixture.root.join("route.json");
    if !route.exists() {
        write(&route, &fixture.route);
    }
    let receiver = service::Receiver::open(&fixture.database, &[route]).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, service::router(receiver))
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (address, stop, task)
}

async fn request(address: SocketAddr, body: Vec<u8>, extra: String) -> (u16, Value) {
    tokio::task::spawn_blocking(move || {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(stream, "POST /hooks/jeryu-public HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n{}\r\n", body.len(), extra).unwrap();
        stream.write_all(&body).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (header, body) = response.split_once("\r\n\r\n").unwrap();
        let code = header.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, serde_json::from_str(body).unwrap())
    }).await.unwrap()
}

fn wire_headers(body: &[u8]) -> String {
    format!(
        "X-GitHub-Delivery: {GUID}\r\nX-GitHub-Event: push\r\nX-Hub-Signature-256: {}\r\n",
        signature(body)
    )
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn http_acknowledgement_requires_durable_raw_bytes_and_retries_survive_restart() {
    let fixture = Fixture::new();
    let mut body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    body.extend_from_slice(b"\n  "); // Authenticate and retain the exact encoding.
    runtime().block_on(async {
        for expected in 1..=2 {
            let (address, stop, task) = serve(&fixture).await;
            let (status, response) = request(address, body.clone(), wire_headers(&body)).await;
            assert_eq!(status, 202);
            assert_eq!(response["publication_qualified"], false);
            assert_eq!(response["audit_execution_admitted"], false);
            assert_eq!(response["reception_id"], expected);
            let bytes: Vec<u8> = fixture
                .connection
                .query_row(
                    "SELECT body_bytes FROM intake_receptions WHERE id=?1",
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(bytes, body);
            stop.send(()).unwrap();
            task.await.unwrap();
        }
    });
    assert_eq!(fixture.status()["pending_received_events"], 1);
    let receptions: i64 = fixture
        .connection
        .query_row("SELECT count(*) FROM intake_receptions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(receptions, 2);
}

#[test]
fn http_rejects_altered_bodies_duplicate_signatures_and_unsigned_metadata_conflicts() {
    let fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    runtime().block_on(async {
        let (address, stop, task) = serve(&fixture).await;
        let mut altered = body.clone();
        altered.push(b'\n');
        assert_eq!(request(address, altered, wire_headers(&body)).await.0, 400);
        let duplicate = format!(
            "{}X-Hub-Signature-256: {}\r\n",
            wire_headers(&body),
            signature(&body)
        );
        assert_eq!(request(address, body.clone(), duplicate).await.0, 400);
        let duplicate_event = format!("{}X-GitHub-Event: release\r\n", wire_headers(&body));
        assert_eq!(request(address, body.clone(), duplicate_event).await.0, 400);
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            202
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    });
    let observed: Vec<String> = fixture
        .connection
        .prepare("SELECT authentication FROM intake_receptions ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        observed,
        [
            "signature_mismatch",
            "headers_unavailable_or_invalid",
            "matched",
            "matched"
        ]
    );
    assert_eq!(fixture.status()["pending_received_events"], 1);
}

#[test]
fn receiver_replays_interrupted_classification_and_refuses_changed_database() {
    let mut fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    fixture.capture(&body, headers(&body, "push", GUID));
    let before: i64 = fixture
        .connection
        .query_row("SELECT count(*) FROM intake_classifications", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(before, 0);
    runtime().block_on(async {
        let (address, stop, task) = serve(&fixture).await;
        let replayed: i64 = fixture
            .connection
            .query_row("SELECT count(*) FROM intake_classifications", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(replayed, 1);
        fs::rename(
            &fixture.database,
            fixture.root.join("retained-original.sqlite"),
        )
        .unwrap();
        write(&fixture.database, b"foreign database bytes");
        let (status, response) = request(address, body.clone(), wire_headers(&body)).await;
        assert_eq!(status, 503);
        assert!(response["reception_id"].is_null());
        assert_eq!(
            fs::read(&fixture.database).unwrap(),
            b"foreign database bytes"
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    });
}

#[test]
fn receiver_holds_route_key_until_restart_and_refuses_unsafe_configuration() {
    let fixture = Fixture::new();
    let route = fixture.root.join("route.json");
    write(&route, &fixture.route);
    assert!(service::Receiver::open(&fixture.database, &[route.clone(), route.clone()]).is_err());
    fs::set_permissions(&route, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(service::Receiver::open(&fixture.database, std::slice::from_ref(&route)).is_err());
    fs::set_permissions(&route, fs::Permissions::from_mode(0o600)).unwrap();
    runtime().block_on(async {
        let (address, stop, task) = serve(&fixture).await;
        fs::write(fixture.root.join("secret"), b"replacement key").unwrap();
        let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            202
        );
        stop.send(()).unwrap();
        task.await.unwrap();
        let (address, stop, task) = serve(&fixture).await;
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            400
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    });
}

#[test]
fn failed_durable_writes_never_acknowledge_and_committed_receptions_replay() {
    let fixture = Fixture::new();
    let body = push(&"a".repeat(40), &"b".repeat(40), "refs/heads/main");
    runtime().block_on(async {
        let (address, stop, task) = serve(&fixture).await;
        fixture
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_reception BEFORE INSERT ON intake_receptions
             BEGIN SELECT RAISE(ABORT,'synthetic reception write failure'); END;",
            )
            .unwrap();
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            503
        );
        let raw: i64 = fixture
            .connection
            .query_row("SELECT count(*) FROM intake_receptions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(raw, 0);
        fixture
            .connection
            .execute_batch(
                "DROP TRIGGER reject_reception;
             CREATE TRIGGER reject_classification BEFORE INSERT ON intake_classifications
             BEGIN SELECT RAISE(ABORT,'synthetic classification write failure'); END;",
            )
            .unwrap();
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            503
        );
        let raw: Vec<u8> = fixture
            .connection
            .query_row("SELECT body_bytes FROM intake_receptions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(raw, body);
        stop.send(()).unwrap();
        task.await.unwrap();
        fixture
            .connection
            .execute_batch("DROP TRIGGER reject_classification;")
            .unwrap();
        let (address, stop, task) = serve(&fixture).await;
        let classifications: i64 = fixture
            .connection
            .query_row("SELECT count(*) FROM intake_classifications", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(classifications, 1);
        assert_eq!(
            request(address, body.clone(), wire_headers(&body)).await.0,
            202
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    });
    assert_eq!(fixture.status()["pending_received_events"], 1);
}
